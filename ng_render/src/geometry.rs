use std::ffi::CStr;
use std::sync::Arc;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use vk_shader_macros::include_glsl;

use crate::{
    guard::{GuardableResource, Guarded},
    shared::{SharedFrond, SharedStem},
    util::create_shader_module,
};

pub struct GeometryStem {
    pipeline_layout: vk::PipelineLayout,
    shared_stem: Arc<SharedStem>,
    triangle_frag_shader_module: vk::ShaderModule,
    triangle_vert_shader_module: vk::ShaderModule,
}

impl GeometryStem {
    pub fn new(shared_stem: Arc<SharedStem>) -> VkResult<Self> {
        unsafe {
            let device = shared_stem.device();

            let pipeline_layout = device
                .create_pipeline_layout(&Default::default(), None)?
                .guard_with(device);
            shared_stem.set_name(*pipeline_layout, "geometry")?;

            let triangle_vert_shader_module =
                create_shader_module(device, include_glsl!("shaders/triangle.vert"))?;
            shared_stem.set_name(*triangle_vert_shader_module, "triangle vert")?;
            let triangle_frag_shader_module =
                create_shader_module(device, include_glsl!("shaders/triangle.frag"))?;
            shared_stem.set_name(*triangle_frag_shader_module, "triangle frag")?;

            Ok(Self {
                pipeline_layout: pipeline_layout.take(),
                triangle_frag_shader_module: triangle_frag_shader_module.take(),
                triangle_vert_shader_module: triangle_vert_shader_module.take(),
                shared_stem,
            })
        }
    }
}

impl Drop for GeometryStem {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_stem.device();
            let _ = device.device_wait_idle();

            device.destroy_shader_module(self.triangle_vert_shader_module, None);
            device.destroy_shader_module(self.triangle_frag_shader_module, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

pub struct GeometryFrond {
    framebuffer: vk::Framebuffer,
    pipeline: vk::Pipeline,
    render_pass: vk::RenderPass,
    shared_frond: Arc<SharedFrond>,
    _geometry_stem: Arc<GeometryStem>,
}

impl GeometryFrond {
    pub fn new(geometry_stem: Arc<GeometryStem>, shared_frond: Arc<SharedFrond>) -> VkResult<Self> {
        let shared_stem = &geometry_stem.shared_stem;
        shared_stem.assert_is(&shared_frond.stem());
        unsafe {
            let device = shared_frond.device();

            let render_pass = Self::create_render_pass(device, shared_frond.diffuse().format)?;
            shared_stem.set_name(*render_pass, "geometry")?;

            let pipeline = Self::create_pipeline(
                device,
                geometry_stem.triangle_vert_shader_module,
                geometry_stem.triangle_frag_shader_module,
                shared_frond.resolution(),
                geometry_stem.pipeline_layout,
                *render_pass,
            )?;
            shared_stem.set_name(*pipeline, "geometry")?;

            let framebuffer = Self::create_framebuffer(
                device,
                *render_pass,
                shared_frond.diffuse().view,
                shared_frond.resolution(),
            )?;
            shared_stem.set_name(*framebuffer, "geometry")?;

            Ok(Self {
                framebuffer: framebuffer.take(),
                pipeline: pipeline.take(),
                render_pass: render_pass.take(),
                shared_frond,
                _geometry_stem: geometry_stem,
            })
        }
    }

    unsafe fn create_render_pass(
        device: &ash::Device,
        format: vk::Format,
    ) -> VkResult<Guarded<(vk::RenderPass, &ash::Device)>> {
        let attachments = [vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build()];

        let color_attachment_refs = [vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build()];
        let subpasses = [vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_refs)
            .build()];

        let dependencies = [];

        let render_pass_create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        Ok(device
            .create_render_pass(&render_pass_create_info, None)?
            .guard_with(device))
    }

    unsafe fn create_pipeline(
        device: &ash::Device,
        triangle_vert_shader_module: vk::ShaderModule,
        triangle_frag_shader_module: vk::ShaderModule,
        resolution: vk::Extent2D,
        pipeline_layout: vk::PipelineLayout,
        render_pass: vk::RenderPass,
    ) -> VkResult<Guarded<(vk::Pipeline, &ash::Device)>> {
        let entry_point = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let vert_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(triangle_vert_shader_module)
            .name(entry_point)
            .stage(vk::ShaderStageFlags::VERTEX);
        let frag_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(triangle_frag_shader_module)
            .name(entry_point)
            .stage(vk::ShaderStageFlags::FRAGMENT);
        let shader_stages = [*vert_create_info, *frag_create_info];

        let vertex_input_state = Default::default();

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: resolution.width as _,
            height: resolution.height as _,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: resolution,
        }];
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let attachments = [vk::PipelineColorBlendAttachmentState {
            color_write_mask: vk::ColorComponentFlags::all(),
            ..Default::default()
        }];
        let color_blend_state =
            vk::PipelineColorBlendStateCreateInfo::builder().attachments(&attachments);

        let graphics_pipeline_create_infos = [vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            // .tesselation_state()
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            //.depth_stencil_state()
            .color_blend_state(&color_blend_state)
            // .dynamic_state()
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0)
            .build()];

        let mut pipelines = device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &graphics_pipeline_create_infos,
                None,
            )
            .map_err(|(_, err)| err)?;

        Ok(pipelines.pop().unwrap().guard_with(device))
    }

    unsafe fn create_framebuffer(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        image_view: vk::ImageView,
        resolution: vk::Extent2D,
    ) -> VkResult<Guarded<(vk::Framebuffer, &ash::Device)>> {
        let attachments = [image_view];
        let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(resolution.width)
            .height(resolution.height)
            .layers(1);
        Ok(device
            .create_framebuffer(&framebuffer_create_info, None)?
            .guard_with(device))
    }

    pub unsafe fn draw(&self, command_buffer: vk::CommandBuffer) {
        let device = self.shared_frond.device();

        let render_area = vk::Rect2D {
            offset: Default::default(),
            extent: self.shared_frond.resolution(),
        };

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        }];

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffer)
            .render_area(render_area)
            .clear_values(&clear_values);
        device.cmd_begin_render_pass(
            command_buffer,
            &render_pass_begin_info,
            vk::SubpassContents::INLINE,
        );

        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline,
        );

        device.cmd_draw(
            command_buffer,
            3, // vertices
            1, // instances
            0, // first vertex
            0, // first instance
        );

        device.cmd_end_render_pass(command_buffer);
    }
}

impl Drop for GeometryFrond {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_frond.device();
            let _ = device.device_wait_idle();

            device.destroy_framebuffer(self.framebuffer, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

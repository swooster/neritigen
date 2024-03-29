use std::ffi::CStr;
use std::sync::Arc;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use vk_shader_macros::include_glsl;

use crate::{
    guard::{GuardableResource, Guarded},
    shared::{SharedFrond, SharedStem},
    util,
};

pub struct TonemappingStem {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    shared_stem: Arc<SharedStem>,
    frag_shader_module: vk::ShaderModule,
}

impl TonemappingStem {
    pub fn new(shared_stem: Arc<SharedStem>) -> VkResult<Self> {
        unsafe {
            let device = shared_stem.device();

            let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;
            shared_stem.set_name(*descriptor_set_layout, "tonemapping")?;

            let pipeline_layout = util::create_pipeline_layout(
                device,
                &[*descriptor_set_layout],
                &[], // push constant ranges
            )?;
            shared_stem.set_name(*pipeline_layout, "tonemapping")?;

            let frag_shader_module =
                util::create_shader_module(device, include_glsl!("shaders/tonemapping.frag"))?;
            shared_stem.set_name(*frag_shader_module, "tonemapping frag")?;

            Ok(Self {
                descriptor_set_layout: descriptor_set_layout.take(),
                pipeline_layout: pipeline_layout.take(),
                frag_shader_module: frag_shader_module.take(),
                shared_stem,
            })
        }
    }

    unsafe fn create_descriptor_set_layout(
        device: &ash::Device,
    ) -> VkResult<Guarded<(vk::DescriptorSetLayout, &ash::Device)>> {
        let bindings = [vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build()];
        let descriptor_set_layout_create_info =
            vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        Ok(device
            .create_descriptor_set_layout(&descriptor_set_layout_create_info, None)?
            .guard_with(device))
    }
}

impl Drop for TonemappingStem {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_stem.device();
            let _ = device.device_wait_idle();

            device.destroy_shader_module(self.frag_shader_module, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

pub struct TonemappingFrond {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline: vk::Pipeline,
    render_pass: vk::RenderPass,
    shared_frond: Arc<SharedFrond>,
    tonemapping_stem: Arc<TonemappingStem>,
}

impl TonemappingFrond {
    pub fn new(
        tonemapping_stem: Arc<TonemappingStem>,
        shared_frond: Arc<SharedFrond>,
    ) -> VkResult<Self> {
        let shared_stem = &tonemapping_stem.shared_stem;
        shared_stem.assert_is(&shared_frond.stem());
        unsafe {
            let device = shared_frond.device();

            let descriptor_pool = util::create_descriptor_pool(
                device,
                1,
                &[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::INPUT_ATTACHMENT,
                    descriptor_count: 1,
                }],
            )?;
            shared_stem.set_name(*descriptor_pool, "tonemapping")?;

            let descriptor_set = Self::allocate_descriptor_set(
                device,
                *descriptor_pool,
                tonemapping_stem.descriptor_set_layout,
                shared_frond.light().view,
            )?;
            shared_stem.set_name(descriptor_set, "tonemapping")?;

            let render_pass = Self::create_render_pass(
                device,
                shared_frond.light().format,
                shared_frond.swapchain_format(),
            )?;
            shared_stem.set_name(*render_pass, "tonemapping")?;

            let pipeline = Self::create_pipeline(
                device,
                shared_frond.stem().fullscreen_vert_shader_module(),
                tonemapping_stem.frag_shader_module,
                shared_frond.resolution(),
                tonemapping_stem.pipeline_layout,
                *render_pass,
            )?;
            shared_stem.set_name(*pipeline, "tonemapping")?;

            let framebuffers = Self::create_framebuffers(
                device,
                *render_pass,
                shared_frond.light().view,
                shared_frond.swapchain_image_views(),
                shared_frond.resolution(),
            )?;
            for framebuffer in framebuffers.iter() {
                shared_stem.set_name(*framebuffer, "tonemapping")?;
            }

            Ok(Self {
                descriptor_pool: descriptor_pool.take(),
                framebuffers: framebuffers.take(),
                pipeline: pipeline.take(),
                render_pass: render_pass.take(),
                descriptor_set,
                shared_frond,
                tonemapping_stem,
            })
        }
    }

    unsafe fn allocate_descriptor_set(
        device: &ash::Device,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        light_view: vk::ImageView,
    ) -> VkResult<vk::DescriptorSet> {
        let set_layouts = [descriptor_set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set = device.allocate_descriptor_sets(&allocate_info)?[0];

        let image_info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: light_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let descriptor_writes = [vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
            .image_info(&image_info)
            .build()];
        device.update_descriptor_sets(&descriptor_writes, &[]);

        Ok(descriptor_set)
    }

    unsafe fn create_render_pass(
        device: &ash::Device,
        light_format: vk::Format,
        swapchain_format: vk::Format,
    ) -> VkResult<Guarded<(vk::RenderPass, &ash::Device)>> {
        let attachments = [
            vk::AttachmentDescription::builder()
                .format(light_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .build(),
            vk::AttachmentDescription::builder()
                .format(swapchain_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .build(),
        ];

        let input_attachments = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let color_attachments = [vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let subpasses = [vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .input_attachments(&input_attachments)
            .color_attachments(&color_attachments)
            .build()];

        let dependencies = [vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::INPUT_ATTACHMENT_READ)
            .build()];

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

    unsafe fn create_framebuffers<'a>(
        device: &'a ash::Device,
        render_pass: vk::RenderPass,
        light_view: vk::ImageView,
        image_views: &[vk::ImageView],
        resolution: vk::Extent2D,
    ) -> VkResult<Guarded<(Vec<vk::Framebuffer>, &'a ash::Device)>> {
        let mut framebuffers = Vec::<vk::Framebuffer>::new().guard_with(device);
        for &image_view in image_views {
            let attachments = [light_view, image_view];
            let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(resolution.width)
                .height(resolution.height)
                .layers(1);
            framebuffers.push(device.create_framebuffer(&framebuffer_create_info, None)?)
        }
        Ok(framebuffers)
    }

    pub unsafe fn draw(&self, command_buffer: vk::CommandBuffer, image_index: u32) {
        let device = self.shared_frond.device();

        let render_area = vk::Rect2D {
            offset: Default::default(),
            extent: self.shared_frond.resolution(),
        };

        let clear_values = [Default::default(), Default::default()];

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index as usize])
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

        device.cmd_bind_descriptor_sets(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            self.tonemapping_stem.pipeline_layout,
            0,
            &[self.descriptor_set],
            &[],
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

impl Drop for TonemappingFrond {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_frond.device();
            let _ = device.device_wait_idle();

            for &framebuffer in self.framebuffers.iter() {
                device.destroy_framebuffer(framebuffer, None);
            }
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_render_pass(self.render_pass, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

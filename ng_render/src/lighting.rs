use std::ffi::CStr;
use std::sync::Arc;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use crevice::std140::{AsStd140, Std140};
use mint;
use nalgebra as na;
use vk_shader_macros::include_glsl;

use crate::{
    guard::{GuardableResource, Guarded},
    shared::{SharedFrond, SharedStem},
    util,
};

#[derive(AsStd140)]
struct LightBuffer {
    pub screen_to_shadow: mint::ColumnMatrix4<f32>,
    pub sunlight_direction: mint::Vector4<f32>,
    pub shadow_size: i32,
}

impl LightBuffer {
    pub fn push_constant_range() -> vk::PushConstantRange {
        vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: Self::std140_size_static() as _,
        }
    }
}

pub struct LightingStem {
    descriptor_set_layout: vk::DescriptorSetLayout,
    lighting_frag_shader_module: vk::ShaderModule,
    pipeline_layout: vk::PipelineLayout,
    shadow_sampler: vk::Sampler,
    volumetric_frag_shader_module: vk::ShaderModule,
    volumetric_vert_shader_module: vk::ShaderModule,
    shared_stem: Arc<SharedStem>,
}

impl LightingStem {
    pub fn new(shared_stem: Arc<SharedStem>) -> VkResult<Self> {
        unsafe {
            let device = shared_stem.device();

            let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;
            shared_stem.set_name(*descriptor_set_layout, "lighting")?;

            let pipeline_layout = util::create_pipeline_layout(
                device,
                &[*descriptor_set_layout],
                &[LightBuffer::push_constant_range()],
            )?;
            shared_stem.set_name(*pipeline_layout, "lighting")?;

            let volumetric_vert_shader_module =
                util::create_shader_module(device, include_glsl!("shaders/volumetric.vert"))?;
            shared_stem.set_name(*volumetric_vert_shader_module, "volumetric vert")?;

            let volumetric_frag_shader_module =
                util::create_shader_module(device, include_glsl!("shaders/volumetric.frag"))?;
            shared_stem.set_name(*volumetric_frag_shader_module, "volumetric frag")?;

            let lighting_frag_shader_module =
                util::create_shader_module(device, include_glsl!("shaders/lighting.frag"))?;
            shared_stem.set_name(*lighting_frag_shader_module, "lighting frag")?;

            let shadow_sampler = Self::create_sampler(device)?;
            shared_stem.set_name(*shadow_sampler, "shadow")?;

            Ok(Self {
                descriptor_set_layout: descriptor_set_layout.take(),
                lighting_frag_shader_module: lighting_frag_shader_module.take(),
                pipeline_layout: pipeline_layout.take(),
                shadow_sampler: shadow_sampler.take(),
                volumetric_frag_shader_module: volumetric_frag_shader_module.take(),
                volumetric_vert_shader_module: volumetric_vert_shader_module.take(),
                shared_stem,
            })
        }
    }

    unsafe fn create_descriptor_set_layout(
        device: &ash::Device,
    ) -> VkResult<Guarded<(vk::DescriptorSetLayout, &ash::Device)>> {
        let bindings = [
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];
        let descriptor_set_layout_create_info =
            vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        Ok(device
            .create_descriptor_set_layout(&descriptor_set_layout_create_info, None)?
            .guard_with(device))
    }

    unsafe fn create_sampler(
        device: &ash::Device,
    ) -> VkResult<Guarded<(vk::Sampler, &ash::Device)>> {
        let sampler_create_info = vk::SamplerCreateInfo::builder()
            // .flags()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            // .mipmap_lod_bias()
            // .anisotropy_enable()
            // .max_anisotropy()
            .compare_enable(false)
            .compare_op(vk::CompareOp::GREATER_OR_EQUAL)
            // .compare_op(vk::CompareOp::LESS)
            .min_lod(0.0)
            .max_lod(vk::LOD_CLAMP_NONE)
            // .border_color()
            .unnormalized_coordinates(false);
        Ok(device
            .create_sampler(&sampler_create_info, None)?
            .guard_with(device))
    }
}

impl Drop for LightingStem {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_stem.device();
            let _ = device.device_wait_idle();

            device.destroy_sampler(self.shadow_sampler, None);
            device.destroy_shader_module(self.volumetric_frag_shader_module, None);
            device.destroy_shader_module(self.volumetric_vert_shader_module, None);
            device.destroy_shader_module(self.lighting_frag_shader_module, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

pub struct LightingFrond {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    framebuffer: vk::Framebuffer,
    lighting_pipeline: vk::Pipeline,
    render_pass: vk::RenderPass,
    volumetric_pipeline: vk::Pipeline,
    shared_frond: Arc<SharedFrond>,
    lighting_stem: Arc<LightingStem>,
}

impl LightingFrond {
    pub fn new(lighting_stem: Arc<LightingStem>, shared_frond: Arc<SharedFrond>) -> VkResult<Self> {
        let shared_stem = &lighting_stem.shared_stem;
        shared_stem.assert_is(&shared_frond.stem());
        unsafe {
            let device = shared_frond.device();

            let descriptor_pool = util::create_descriptor_pool(
                device,
                1,
                &[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::INPUT_ATTACHMENT,
                        descriptor_count: 3,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 1,
                    },
                ],
            )?;
            shared_stem.set_name(*descriptor_pool, "lighting")?;

            let descriptor_set = Self::allocate_descriptor_set(
                device,
                *descriptor_pool,
                lighting_stem.descriptor_set_layout,
                shared_frond.diffuse().view,
                shared_frond.normal().view,
                shared_frond.depth_stencil().view,
                shared_frond.shadow().view,
                lighting_stem.shadow_sampler,
            )?;
            shared_stem.set_name(descriptor_set, "lighting")?;

            let render_pass = Self::create_render_pass(
                device,
                shared_frond.diffuse().format,
                shared_frond.normal().format,
                shared_frond.depth_stencil().format,
                shared_frond.light().format,
            )?;
            shared_stem.set_name(*render_pass, "lighting")?;

            let volumetric_pipeline = Self::create_volumetric_pipeline(
                device,
                lighting_stem.volumetric_vert_shader_module,
                lighting_stem.volumetric_frag_shader_module,
                shared_frond.resolution(),
                lighting_stem.pipeline_layout,
                *render_pass,
            )?;
            shared_stem.set_name(*volumetric_pipeline, "volumetric")?;

            let lighting_pipeline = Self::create_lighting_pipeline(
                device,
                shared_frond.stem().fullscreen_vert_shader_module(),
                lighting_stem.lighting_frag_shader_module,
                shared_frond.resolution(),
                lighting_stem.pipeline_layout,
                *render_pass,
            )?;
            shared_stem.set_name(*lighting_pipeline, "lighting")?;

            let framebuffer = util::create_framebuffer(
                device,
                *render_pass,
                &[
                    shared_frond.diffuse().view,
                    shared_frond.normal().view,
                    shared_frond.depth_stencil().view,
                    shared_frond.light().view,
                ],
                shared_frond.resolution(),
            )?;
            shared_stem.set_name(*framebuffer, "lighting")?;

            Ok(Self {
                descriptor_pool: descriptor_pool.take(),
                framebuffer: framebuffer.take(),
                lighting_pipeline: lighting_pipeline.take(),
                render_pass: render_pass.take(),
                volumetric_pipeline: volumetric_pipeline.take(),
                descriptor_set,
                shared_frond,
                lighting_stem,
            })
        }
    }

    unsafe fn allocate_descriptor_set(
        device: &ash::Device,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        diffuse_view: vk::ImageView,
        normal_view: vk::ImageView,
        depth_view: vk::ImageView,
        shadow_view: vk::ImageView,
        shadow_sampler: vk::Sampler,
    ) -> VkResult<vk::DescriptorSet> {
        let set_layouts = [descriptor_set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set = device.allocate_descriptor_sets(&allocate_info)?[0];

        let diffuse_info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: diffuse_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let normal_info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: normal_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let depth_info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: depth_view,
            image_layout: vk::ImageLayout::GENERAL, // TODO: vulkan 1.2 so I can do DEPTH_READ_ONLY_STENCIL_ATTACHMENT_OPTIMAL
        }];
        let shadow_info = [vk::DescriptorImageInfo {
            sampler: shadow_sampler,
            image_view: shadow_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let descriptor_writes = [
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .image_info(&diffuse_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .image_info(&normal_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::INPUT_ATTACHMENT)
                .image_info(&depth_info)
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&shadow_info)
                .build(),
        ];
        device.update_descriptor_sets(&descriptor_writes, &[]);

        Ok(descriptor_set)
    }

    unsafe fn create_render_pass(
        device: &ash::Device,
        diffuse_format: vk::Format,
        normal_format: vk::Format,
        depth_format: vk::Format,
        light_format: vk::Format,
    ) -> VkResult<Guarded<(vk::RenderPass, &ash::Device)>> {
        let attachments = [
            vk::AttachmentDescription::builder()
                .format(diffuse_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .build(),
            vk::AttachmentDescription::builder()
                .format(normal_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .build(),
            vk::AttachmentDescription::builder()
                .format(depth_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::GENERAL)
                .build(),
            vk::AttachmentDescription::builder()
                .format(light_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .build(),
        ];

        let input_attachments = [
            vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
            vk::AttachmentReference {
                attachment: 1,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            },
            vk::AttachmentReference {
                attachment: 2,
                layout: vk::ImageLayout::GENERAL,
            },
        ];
        let color_attachments = [vk::AttachmentReference {
            attachment: 3,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_stencil_attachment = vk::AttachmentReference {
            attachment: 2,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let depth_stencil_attachment_general = vk::AttachmentReference {
            attachment: 2,
            layout: vk::ImageLayout::GENERAL,
        };
        let subpasses = [
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .input_attachments(&[])
                .color_attachments(&color_attachments)
                .depth_stencil_attachment(&depth_stencil_attachment)
                .build(),
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .input_attachments(&input_attachments)
                .color_attachments(&color_attachments)
                .depth_stencil_attachment(&depth_stencil_attachment_general)
                .build(),
        ];

        let dependencies = [
            // Subpass 0:
            // Make shadow depth available for vertex positioning
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::LATE_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_subpass(0)
                .dst_stage_mask(vk::PipelineStageFlags::VERTEX_SHADER)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .build(),
            // Make geometry depth available for light-volume-surface intersection
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::LATE_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_subpass(0)
                .dst_stage_mask(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ)
                .build(),
            // Subpass 1
            // Make diffuse/normal/etc available for input attachments
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_subpass(1)
                .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags::INPUT_ATTACHMENT_READ)
                .build(),
            // Make light buffer changes available for blending
            vk::SubpassDependency::builder()
                .src_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_subpass(1)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ)
                .build(),
            // Make stencil changes by subpass 0 available
            vk::SubpassDependency::builder()
                .src_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::LATE_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_subpass(1)
                .dst_stage_mask(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ)
                .build(),
        ];

        let render_pass_create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        Ok(device
            .create_render_pass(&render_pass_create_info, None)?
            .guard_with(device))
    }

    unsafe fn create_volumetric_pipeline(
        device: &ash::Device,
        volumetric_vert_shader_module: vk::ShaderModule,
        volumetric_frag_shader_module: vk::ShaderModule,
        resolution: vk::Extent2D,
        pipeline_layout: vk::PipelineLayout,
        render_pass: vk::RenderPass,
    ) -> VkResult<Guarded<(vk::Pipeline, &ash::Device)>> {
        let entry_point = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let vert_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(volumetric_vert_shader_module)
            .name(entry_point)
            .stage(vk::ShaderStageFlags::VERTEX);
        let frag_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(volumetric_frag_shader_module)
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

        let stencil_op_state = vk::StencilOpState {
            fail_op: vk::StencilOp::INVERT,
            pass_op: vk::StencilOp::INVERT,
            depth_fail_op: vk::StencilOp::INVERT,
            compare_op: vk::CompareOp::ALWAYS,
            compare_mask: 0,
            write_mask: 1,
            reference: 0,
        };
        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(false)
            .depth_compare_op(vk::CompareOp::GREATER)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(true)
            .front(stencil_op_state)
            .back(stencil_op_state)
            //.min_depth_bounds()
            //.max_depth_bounds()
            ;

        let attachments = [vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_DST_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::all())
            .build()];
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
            .depth_stencil_state(&depth_stencil_state)
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

    unsafe fn create_lighting_pipeline(
        device: &ash::Device,
        lighting_vert_shader_module: vk::ShaderModule,
        lighting_frag_shader_module: vk::ShaderModule,
        resolution: vk::Extent2D,
        pipeline_layout: vk::PipelineLayout,
        render_pass: vk::RenderPass,
    ) -> VkResult<Guarded<(vk::Pipeline, &ash::Device)>> {
        let entry_point = CStr::from_bytes_with_nul(b"main\0").unwrap();
        let vert_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(lighting_vert_shader_module)
            .name(entry_point)
            .stage(vk::ShaderStageFlags::VERTEX);
        let frag_create_info = vk::PipelineShaderStageCreateInfo::builder()
            .module(lighting_frag_shader_module)
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

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(false)
            .depth_write_enable(false)
            .depth_compare_op(vk::CompareOp::GREATER)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false)
            //.front()
            //.back()
            //.min_depth_bounds()
            //.max_depth_bounds()
            ;

        let attachments = [vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::all())
            .build()];
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
            .depth_stencil_state(&depth_stencil_state)
            .color_blend_state(&color_blend_state)
            // .dynamic_state()
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(1)
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

    pub unsafe fn draw(
        &self,
        command_buffer: vk::CommandBuffer,
        view: mint::ColumnMatrix4<f32>,
        draw_shadow: impl Fn(mint::ColumnMatrix4<f32>) -> (),
    ) {
        let device = self.shared_frond.device();

        let sunlight_to_world: na::Matrix4<f32> = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.5, 1.0, 2.0, 0.0],
            [-0.25, -0.5, -1.0, 1.0],
        ]
        .into();
        let world_to_sunlight = sunlight_to_world.try_inverse().unwrap();
        draw_shadow(world_to_sunlight.into());

        let view: na::Matrix4<f32> = view.into();
        let shadow_to_screen = view * sunlight_to_world;
        let screen_to_shadow = world_to_sunlight * view.try_inverse().unwrap();

        let sunlight_direction =
            (sunlight_to_world * na::Vector4::new(0.0, 0.0, -1.0, 0.0)).normalize();

        let shadow_size = self.shared_frond.shadow().resolution.width;

        let render_area = vk::Rect2D {
            offset: Default::default(),
            extent: self.shared_frond.resolution(),
        };

        let clear_values = [
            Default::default(),
            Default::default(),
            Default::default(),
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            },
        ];

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

        device.cmd_bind_descriptor_sets(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            self.lighting_stem.pipeline_layout,
            0,
            &[self.descriptor_set],
            &[],
        );

        let light_buffer = LightBuffer {
            // FIXME: need better name if I'm going to use this two different ways
            screen_to_shadow: shadow_to_screen.into(),
            sunlight_direction: sunlight_direction.into(),
            shadow_size: shadow_size as _,
        };
        device.cmd_push_constants(
            command_buffer,
            self.lighting_stem.pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            light_buffer.as_std140().as_bytes(),
        );

        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            self.volumetric_pipeline,
        );

        device.cmd_draw(
            command_buffer,
            6 * shadow_size * shadow_size + 24 * shadow_size - 12, // vertices
            1,                                                     // instances
            0,                                                     // first vertex
            0,                                                     // first instance
        );

        device.cmd_next_subpass(command_buffer, vk::SubpassContents::INLINE);

        let light_buffer = LightBuffer {
            screen_to_shadow: screen_to_shadow.into(),
            sunlight_direction: sunlight_direction.into(),
            shadow_size: shadow_size as _,
        };
        device.cmd_push_constants(
            command_buffer,
            self.lighting_stem.pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            light_buffer.as_std140().as_bytes(),
        );

        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            self.lighting_pipeline,
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

impl Drop for LightingFrond {
    fn drop(&mut self) {
        unsafe {
            let device = self.shared_frond.device();
            let _ = device.device_wait_idle();

            device.destroy_framebuffer(self.framebuffer, None);
            device.destroy_pipeline(self.volumetric_pipeline, None);
            device.destroy_pipeline(self.lighting_pipeline, None);
            device.destroy_render_pass(self.render_pass, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

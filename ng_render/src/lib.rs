use std::sync::Arc;

use ash::{
    extensions::{khr::Surface, khr::Swapchain},
    version::DeviceV1_0,
    vk,
};
use winit::window::Window;

mod shared;

use shared::{Queues, SharedCrown, SharedStem};

pub struct Renderer {
    framebuffers: Vec<vk::Framebuffer>,
    render_pass: vk::RenderPass,
    resolution: vk::Extent2D,
    shared_stem: SharedStem,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: Vec<vk::ImageView>,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        let shared_crown = SharedCrown::new(window.clone());
        let shared_stem = SharedStem::new(shared_crown);

        let device = shared_stem.device();
        let physical_device = shared_stem.physical_device();
        let queues = shared_stem.queues();
        let surface = shared_stem.surface();
        let surface_fn = shared_stem.surface_fn();
        let swapchain_fn = shared_stem.swapchain_fn();

        let winit::dpi::PhysicalSize { width, height } = window.inner_size();
        let resolution = vk::Extent2D { width, height };

        unsafe {
            let surface_format = Self::select_surface_format(surface_fn, physical_device, surface);

            let render_pass = Self::create_render_pass(&device, surface_format.format);

            let old_swapchain = vk::SwapchainKHR::null();
            let swapchain = Self::create_swapchain(
                surface_fn,
                swapchain_fn,
                physical_device,
                surface,
                surface_format,
                resolution,
                queues,
                old_swapchain,
            );

            let swapchain_image_views = Self::create_swapchain_image_views(
                swapchain_fn,
                device,
                swapchain,
                surface_format.format,
            );

            let framebuffers =
                Self::create_framebuffers(device, render_pass, &swapchain_image_views, resolution);

            Self {
                framebuffers,
                render_pass,
                resolution,
                shared_stem,
                swapchain,
                swapchain_image_views,
            }
        }
    }

    unsafe fn select_surface_format(
        surface_fn: &Surface,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> vk::SurfaceFormatKHR {
        let surface_formats = surface_fn
            .get_physical_device_surface_formats(physical_device, surface)
            .unwrap();
        let desired_formats = [
            vk::SurfaceFormatKHR {
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
                format: vk::Format::B8G8R8A8_SRGB,
            },
            // TODO: Support other formats?
        ];
        *desired_formats
            .iter()
            .find(|&&desired_format| surface_formats.iter().any(|&sfmt| sfmt == desired_format))
            .unwrap()
    }

    unsafe fn create_render_pass(
        device: &ash::Device,
        surface_format: vk::Format,
    ) -> vk::RenderPass {
        let attachments = [vk::AttachmentDescription::builder()
            .format(surface_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
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
        device
            .create_render_pass(&render_pass_create_info, None)
            .unwrap()
    }

    unsafe fn create_swapchain(
        surface_fn: &Surface,
        swapchain_fn: &Swapchain,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
        surface_format: vk::SurfaceFormatKHR,
        default_resolution: vk::Extent2D,
        queues: &Queues,
        old_swapchain: vk::SwapchainKHR,
    ) -> vk::SwapchainKHR {
        let surface_capabilities = surface_fn
            .get_physical_device_surface_capabilities(physical_device, surface)
            .unwrap();

        let max_image_count = match surface_capabilities.max_image_count {
            0 => u32::MAX,
            x => x,
        };
        let min_image_count = (surface_capabilities.min_image_count + 1).min(max_image_count);

        let image_extent = match surface_capabilities.current_extent {
            vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            } => default_resolution,
            x => x,
        };

        let transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };

        let present_mode = surface_fn
            .get_physical_device_surface_present_modes(physical_device, surface)
            .unwrap()
            .into_iter()
            .find(|&m| m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let queue_families = [queues.graphics_family, queues.present_family];
        let (image_sharing_mode, queue_families) =
            if queues.graphics_family == queues.present_family {
                (vk::SharingMode::EXCLUSIVE, &queue_families[..1])
            } else {
                (vk::SharingMode::CONCURRENT, &queue_families[..])
            };

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(min_image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(image_extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(image_sharing_mode)
            .queue_family_indices(queue_families)
            .pre_transform(transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);

        swapchain_fn
            .create_swapchain(&swapchain_create_info, None)
            .unwrap()
    }

    unsafe fn create_swapchain_image_views(
        swapchain_fn: &Swapchain,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        format: vk::Format,
    ) -> Vec<vk::ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1)
            .build();

        swapchain_fn
            .get_swapchain_images(swapchain)
            .unwrap()
            .iter()
            .map(|image| {
                let image_view_create_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(subresource_range)
                    .image(*image);
                device
                    .create_image_view(&image_view_create_info, None)
                    .unwrap()
            })
            .collect()
    }

    unsafe fn create_framebuffers(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        image_views: &[vk::ImageView],
        resolution: vk::Extent2D,
    ) -> Vec<vk::Framebuffer> {
        image_views
            .iter()
            .map(|&image_view| {
                let attachments = [image_view];
                let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(resolution.width)
                    .height(resolution.height)
                    .layers(1);
                device
                    .create_framebuffer(&framebuffer_create_info, None)
                    .unwrap()
            })
            .collect()
    }

    pub fn draw(&mut self) {
        let command_buffer = self.shared_stem.command_buffer();
        let device = self.shared_stem.device();
        let image_acquired_semaphore = self.shared_stem.image_acquired_semaphore();
        let presentation_fence = self.shared_stem.presentation_fence();
        let queues = self.shared_stem.queues();
        let render_complete_semaphore = self.shared_stem.render_complete_semaphore();
        let swapchain_fn = self.shared_stem.swapchain_fn();
        unsafe {
            device
                .wait_for_fences(&[presentation_fence], true, u64::MAX)
                .unwrap();
            device.reset_fences(&[presentation_fence]).unwrap();

            let (image_index, suboptimal_acquire) = swapchain_fn
                .acquire_next_image(
                    self.swapchain,
                    u64::MAX,
                    image_acquired_semaphore,
                    vk::Fence::null(),
                )
                .unwrap();
            assert!(!suboptimal_acquire);

            let command_buffer = command_buffer;
            device
                .reset_command_buffer(
                    command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                .unwrap();

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 1.0, 0.25, 1.0],
                },
            }];

            let render_area = vk::Rect2D {
                offset: Default::default(),
                extent: self.resolution,
            };

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

            device.cmd_end_render_pass(command_buffer);

            device.end_command_buffer(command_buffer).unwrap();

            let wait_semaphores = [image_acquired_semaphore];
            let wait_dst_stage_mask = [vk::PipelineStageFlags::TOP_OF_PIPE];
            let command_buffers = [command_buffer];
            let signal_semaphores = [render_complete_semaphore];
            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_dst_stage_mask)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            let submit_infos = [submit_info.build()];
            device
                .queue_submit(queues.graphics, &submit_infos, presentation_fence)
                .unwrap();

            let wait_semaphores = [render_complete_semaphore];
            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(&wait_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_result = swapchain_fn
                .queue_present(queues.present, &present_info)
                .unwrap();
            assert!(!present_result);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let device = self.shared_stem.device();
        let swapchain_fn = self.shared_stem.swapchain_fn();
        unsafe {
            let _ = device.device_wait_idle();

            for &framebuffer in self.framebuffers.iter() {
                device.destroy_framebuffer(framebuffer, None);
            }
            for &image_view in self.swapchain_image_views.iter() {
                device.destroy_image_view(image_view, None);
            }
            swapchain_fn.destroy_swapchain(self.swapchain, None);
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

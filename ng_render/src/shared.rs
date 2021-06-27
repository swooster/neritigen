use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

use ash::{
    extensions::{ext::DebugUtils, khr::Surface, khr::Swapchain},
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk,
};
use winit::window::Window;

pub struct SharedCrown {
    debug_utils_fn: DebugUtils,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface: Mutex<vk::SurfaceKHR>, // swapchain creation needs surface to be host-synchronized
    surface_fn: Surface,
    window: Arc<Window>,
}

impl SharedCrown {
    pub fn new(window: Arc<Window>) -> Self {
        unsafe {
            let entry = ash::Entry::new().unwrap();
            let instance = Self::create_instance(&entry, &window);

            let debug_utils_fn = DebugUtils::new(&entry, &instance);
            let debug_utils_messenger = debug_utils_fn
                .create_debug_utils_messenger(&Self::debug_utils_messenger_create_info(), None)
                .unwrap();

            let surface =
                Mutex::new(ash_window::create_surface(&entry, &instance, &*window, None).unwrap());
            let surface_fn = Surface::new(&entry, &instance);

            Self {
                debug_utils_fn,
                debug_utils_messenger,
                _entry: entry,
                instance,
                surface,
                surface_fn,
                window,
            }
        }
    }

    unsafe fn create_instance(entry: &ash::Entry, window: &Window) -> ash::Instance {
        let application_name = CString::new("Nerigen").unwrap();
        let application_version = vk::make_version(
            env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap(),
            env!("CARGO_PKG_VERSION_MINOR").parse().unwrap(),
            env!("CARGO_PKG_VERSION_PATCH").parse().unwrap(),
        );
        let application_info = vk::ApplicationInfo::builder()
            .application_name(&application_name)
            .application_version(application_version)
            .engine_name(&application_name)
            .engine_version(application_version)
            .api_version(vk::make_version(1, 0, 0));

        let validation_layer = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
        let enabled_layer_names = [validation_layer.as_ptr()];
        let mut enabled_extension_names =
            ash_window::enumerate_required_extensions(window).unwrap();
        enabled_extension_names.push(DebugUtils::name());
        let enabled_extension_names: Vec<_> = enabled_extension_names
            .into_iter()
            .map(|name| name.as_ptr())
            .collect();
        let mut debug_utils_messenger_create_info = Self::debug_utils_messenger_create_info();
        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_info)
            .enabled_layer_names(&enabled_layer_names)
            .enabled_extension_names(&enabled_extension_names)
            .push_next(&mut debug_utils_messenger_create_info);

        entry.create_instance(&create_info, None).unwrap()
    }

    fn debug_utils_messenger_create_info() -> vk::DebugUtilsMessengerCreateInfoEXTBuilder<'static> {
        vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
            .pfn_user_callback(Some(Self::debug_utils_callback))
    }

    unsafe extern "system" fn debug_utils_callback(
        message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        message_types: vk::DebugUtilsMessageTypeFlagsEXT,
        p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        _p_user_data: *mut c_void,
    ) -> u32 {
        let message_severity = match message_severity {
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => log::Level::Debug,
            vk::DebugUtilsMessageSeverityFlagsEXT::INFO => log::Level::Info,
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::Level::Warn,
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::Level::Error,
            _ => log::Level::Error,
        };
        let message = CStr::from_ptr((*p_callback_data).p_message);
        if let Ok(message) = message.to_str() {
            log::log!(message_severity, "{:?}: {}", message_types, message);
        } else {
            log::log!(message_severity, "{:?}: {:?}", message_types, message);
        }
        vk::FALSE
    }

    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }

    pub fn surface(&self) -> &Mutex<vk::SurfaceKHR> {
        &self.surface
    }

    pub fn surface_fn(&self) -> &Surface {
        &self.surface_fn
    }

    pub fn window_resolution(&self) -> vk::Extent2D {
        let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
        vk::Extent2D { width, height }
    }
}

impl Drop for SharedCrown {
    fn drop(&mut self) {
        let surface = self.surface.lock().unwrap();
        unsafe {
            self.surface_fn.destroy_surface(*surface, None);
            self.debug_utils_fn
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub struct SharedStem {
    command_buffer: vk::CommandBuffer,
    command_pool: vk::CommandPool,
    crown: Arc<SharedCrown>,
    device: ash::Device,
    image_acquired_semaphore: vk::Semaphore,
    physical_device: vk::PhysicalDevice,
    presentation_fence: vk::Fence,
    queues: Queues,
    render_complete_semaphore: vk::Semaphore,
    swapchain_fn: Swapchain,
}

impl SharedStem {
    pub fn new(crown: Arc<SharedCrown>) -> Self {
        let instance = crown.instance();
        let surface = crown.surface();
        let surface = surface.lock().unwrap();
        let surface_fn = crown.surface_fn();

        unsafe {
            let (physical_device, device, queues) =
                Self::create_device_and_queues(instance, surface_fn, *surface);

            let swapchain_fn = Swapchain::new(instance, &device);

            let command_pool = Self::create_command_pool(&device, queues.graphics_family);
            let command_buffer = Self::allocate_command_buffer(&device, command_pool);

            let image_acquired_semaphore =
                device.create_semaphore(&Default::default(), None).unwrap();
            let render_complete_semaphore =
                device.create_semaphore(&Default::default(), None).unwrap();

            let signaled_fence_create_info =
                vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            let presentation_fence = device
                .create_fence(&signaled_fence_create_info, None)
                .unwrap();

            drop(surface);

            Self {
                command_buffer,
                command_pool,
                crown,
                device,
                image_acquired_semaphore,
                physical_device,
                presentation_fence,
                queues,
                render_complete_semaphore,
                swapchain_fn,
            }
        }
    }

    unsafe fn create_device_and_queues(
        instance: &ash::Instance,
        surface_fn: &Surface,
        surface: vk::SurfaceKHR,
    ) -> (vk::PhysicalDevice, ash::Device, Queues) {
        let (physical_device, graphics_queue_family, present_queue_family) =
            Self::select_physical_device_and_queue_families(instance, surface_fn, surface);

        let queue_priorities = [1.0];
        let queue_create_infos = [
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_family)
                .queue_priorities(&queue_priorities)
                .build(),
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(present_queue_family)
                .queue_priorities(&queue_priorities)
                .build(),
        ];
        let queue_create_infos = if graphics_queue_family == present_queue_family {
            &queue_create_infos[0..1]
        } else {
            &queue_create_infos
        };

        let validation_layer = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
        let enabled_layer_names = [validation_layer.as_ptr()];

        let enabled_extension_names = [Swapchain::name().as_ptr()];
        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_create_infos)
            .enabled_extension_names(&enabled_extension_names)
            .enabled_layer_names(&enabled_layer_names);
        let device = instance
            .create_device(physical_device, &device_create_info, None)
            .unwrap();

        let queues = Queues {
            graphics: device.get_device_queue(graphics_queue_family, 0),
            graphics_family: graphics_queue_family,
            present: device.get_device_queue(present_queue_family, 0),
            present_family: present_queue_family,
        };

        (physical_device, device, queues)
    }

    unsafe fn select_physical_device_and_queue_families(
        instance: &ash::Instance,
        surface_fn: &Surface,
        surface: vk::SurfaceKHR,
    ) -> (vk::PhysicalDevice, u32, u32) {
        for physical_device in instance.enumerate_physical_devices().unwrap() {
            let queue_families =
                instance.get_physical_device_queue_family_properties(physical_device);
            let graphics_queue = queue_families
                .iter()
                .position(|info| info.queue_flags.contains(vk::QueueFlags::GRAPHICS));
            let present_queue = queue_families.iter().enumerate().position(|(i, _)| {
                surface_fn
                    .get_physical_device_surface_support(physical_device, i as _, surface)
                    .unwrap()
            });
            if let (Some(graphics_queue), Some(present_queue)) = (graphics_queue, present_queue) {
                return (physical_device, graphics_queue as _, present_queue as _);
            }
        }
        panic!("No suitable device found");
    }

    unsafe fn create_command_pool(
        device: &ash::Device,
        queue_family_index: u32,
    ) -> vk::CommandPool {
        let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        device
            .create_command_pool(&command_pool_create_info, None)
            .unwrap()
    }

    unsafe fn allocate_command_buffer(
        device: &ash::Device,
        command_pool: vk::CommandPool,
    ) -> vk::CommandBuffer {
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .command_buffer_count(1);
        device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .unwrap()[0]
    }

    pub fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub fn crown(&self) -> Arc<SharedCrown> {
        self.crown.clone()
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    pub fn image_acquired_semaphore(&self) -> vk::Semaphore {
        self.image_acquired_semaphore
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub fn presentation_fence(&self) -> vk::Fence {
        self.presentation_fence
    }

    pub fn queues(&self) -> &Queues {
        &self.queues
    }

    pub fn render_complete_semaphore(&self) -> vk::Semaphore {
        self.render_complete_semaphore
    }

    pub fn swapchain_fn(&self) -> &Swapchain {
        &self.swapchain_fn
    }
}

impl Drop for SharedStem {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            self.device.destroy_fence(self.presentation_fence, None);
            self.device
                .destroy_semaphore(self.image_acquired_semaphore, None);
            self.device
                .destroy_semaphore(self.render_complete_semaphore, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
    }
}

pub struct Queues {
    pub graphics: vk::Queue,
    pub graphics_family: u32,
    pub present: vk::Queue,
    pub present_family: u32,
}

pub struct SharedFrond {
    framebuffers: Vec<vk::Framebuffer>,
    render_pass: vk::RenderPass,
    resolution: vk::Extent2D,
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: Vec<vk::ImageView>,
}

impl SharedFrond {
    pub fn new(stem: Arc<SharedStem>) -> Self {
        Self::_new(stem, None)
    }

    fn _new(stem: Arc<SharedStem>, old_swapchain: Option<SharedFrondSwapchain>) -> Self {
        let device = stem.device();
        let physical_device = stem.physical_device();
        let queues = stem.queues();
        let swapchain_fn = stem.swapchain_fn();

        let crown = stem.crown();
        let surface = crown.surface();
        let surface = surface.lock().unwrap();
        let surface_fn = crown.surface_fn();
        let resolution = crown.window_resolution();

        unsafe {
            let surface_format = Self::select_surface_format(surface_fn, physical_device, *surface);

            let render_pass = Self::create_render_pass(&device, surface_format.format);

            let old_swapchain = old_swapchain
                .as_ref() // Don't destroy the swapchain before it's used
                .map(|s| s.swapchain())
                .unwrap_or_default();
            let swapchain = Self::create_swapchain(
                surface_fn,
                swapchain_fn,
                physical_device,
                *surface,
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
                stem,
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

    pub fn take_swapchain(mut self) -> SharedFrondSwapchain {
        SharedFrondSwapchain {
            stem: self.stem.clone(),
            swapchain: std::mem::take(&mut self.swapchain),
        }
    }

    pub fn framebuffers(&self) -> &[vk::Framebuffer] {
        &self.framebuffers
    }

    pub fn render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }

    pub fn resolution(&self) -> vk::Extent2D {
        self.resolution
    }

    pub fn stem(&self) -> Arc<SharedStem> {
        self.stem.clone()
    }

    pub fn swapchain(&self) -> vk::SwapchainKHR {
        self.swapchain
    }
}

impl Drop for SharedFrond {
    fn drop(&mut self) {
        let device = self.stem.device();
        let swapchain_fn = self.stem.swapchain_fn();
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

pub struct SharedFrondSwapchain {
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
}

impl SharedFrondSwapchain {
    pub fn ressurect(self) -> Result<SharedFrond, SharedFrondSwapchain> {
        let resolution = self.stem.crown().window_resolution();
        if resolution.width > 0 && resolution.height > 0 {
            Ok(SharedFrond::_new(self.stem.clone(), Some(self)))
        } else {
            Err(self)
        }
    }

    pub fn swapchain(&self) -> vk::SwapchainKHR {
        self.swapchain
    }
}

impl Drop for SharedFrondSwapchain {
    fn drop(&mut self) {
        let device = self.stem.device();
        let swapchain_fn = self.stem.swapchain_fn();

        unsafe {
            let _ = device.device_wait_idle();

            swapchain_fn.destroy_swapchain(self.swapchain, None);
        }
    }
}

use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

use ash::{
    extensions::{ext::DebugUtils, khr::Surface, khr::Swapchain},
    prelude::VkResult,
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk,
};
use thiserror::Error;
use vk_shader_macros::include_glsl;
use winit::window::Window;

use crate::guard::{GuardableResource, Guarded};

pub struct SharedCrown {
    debug_utils_fn: DebugUtils,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface: Mutex<vk::SurfaceKHR>, // swapchain creation needs surface to be host-synchronized
    surface_fn: Surface,
    window: Arc<Window>,
}

#[derive(Error, Debug)]
pub enum SharedCrownError {
    #[error("Vulkan error occurred")]
    VkError(#[from] vk::Result), // TODO: split into contexts
    #[error("Couldn't create Entry")]
    EntryError(#[from] ash::LoadingError),
    #[error("Couldn't create Instance")]
    InstanceError(#[from] ash::InstanceError),
}

impl SharedCrown {
    pub fn new(window: Arc<Window>) -> Result<Self, SharedCrownError> {
        unsafe {
            let entry = ash::Entry::new()?;
            let instance = Self::create_instance(&entry, &window)?;

            let debug_utils_fn = DebugUtils::new(&entry, &*instance);
            let debug_utils_messenger = debug_utils_fn
                .create_debug_utils_messenger(&Self::debug_utils_messenger_create_info(), None)?
                .guard_with(&debug_utils_fn);

            let surface_fn = Surface::new(&entry, &*instance);
            let surface = ash_window::create_surface(&entry, &*instance, &*window, None)?
                .guard_with(&surface_fn);

            Ok(Self {
                debug_utils_messenger: debug_utils_messenger.take(),
                instance: instance.take(),
                surface: Mutex::new(surface.take()),
                debug_utils_fn,
                _entry: entry,
                surface_fn,
                window,
            })
        }
    }

    unsafe fn create_instance(
        entry: &ash::Entry,
        window: &Window,
    ) -> Result<Guarded<ash::Instance>, ash::InstanceError> {
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
        let mut enabled_extension_names = ash_window::enumerate_required_extensions(window)
            .map_err(ash::InstanceError::VkError)?;
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

        Ok(entry.create_instance(&create_info, None)?.guard())
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

#[derive(Error, Debug)]
pub enum SharedStemError {
    #[error("Vulkan error occurred")]
    VkError(#[from] vk::Result), // TODO: split into contexts
    #[error("Couldn't select acceptable graphics device")]
    NoAcceptableDeviceError,
}

impl SharedStem {
    pub fn new(crown: Arc<SharedCrown>) -> Result<Self, SharedStemError> {
        let instance = crown.instance();
        let surface = crown.surface();
        let surface = surface.lock().unwrap();
        let surface_fn = crown.surface_fn();

        unsafe {
            let (physical_device, device, queues) =
                Self::create_device_and_queues(instance, surface_fn, *surface)?;

            let swapchain_fn = Swapchain::new(instance, &*device);

            let command_pool = Self::create_command_pool(&device, queues.graphics_family)?;
            let command_buffer = Self::allocate_command_buffer(&device, *command_pool)?;

            let image_acquired_semaphore = device
                .create_semaphore(&Default::default(), None)?
                .guard_with(&*device);
            let render_complete_semaphore = device
                .create_semaphore(&Default::default(), None)?
                .guard_with(&*device);

            let signaled_fence_create_info =
                vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            let presentation_fence = device
                .create_fence(&signaled_fence_create_info, None)?
                .guard_with(&*device);

            drop(surface);

            Ok(Self {
                command_pool: command_pool.take(),
                image_acquired_semaphore: image_acquired_semaphore.take(),
                presentation_fence: presentation_fence.take(),
                render_complete_semaphore: render_complete_semaphore.take(),
                device: device.take(),
                command_buffer,
                crown,
                physical_device,
                queues,
                swapchain_fn,
            })
        }
    }

    unsafe fn create_device_and_queues(
        instance: &ash::Instance,
        surface_fn: &Surface,
        surface: vk::SurfaceKHR,
    ) -> Result<(vk::PhysicalDevice, Guarded<ash::Device>, Queues), SharedStemError> {
        let (physical_device, graphics_queue_family, present_queue_family) =
            Self::select_physical_device_and_queue_families(instance, surface_fn, surface)?
                .ok_or(SharedStemError::NoAcceptableDeviceError)?;

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
            .create_device(physical_device, &device_create_info, None)?
            .guard();

        let queues = Queues {
            graphics: device.get_device_queue(graphics_queue_family, 0),
            graphics_family: graphics_queue_family,
            present: device.get_device_queue(present_queue_family, 0),
            present_family: present_queue_family,
        };

        Ok((physical_device, device, queues))
    }

    unsafe fn select_physical_device_and_queue_families(
        instance: &ash::Instance,
        surface_fn: &Surface,
        surface: vk::SurfaceKHR,
    ) -> VkResult<Option<(vk::PhysicalDevice, u32, u32)>> {
        for physical_device in instance.enumerate_physical_devices()? {
            let queue_families =
                instance.get_physical_device_queue_family_properties(physical_device);
            let graphics_queue = queue_families
                .iter()
                .position(|info| info.queue_flags.contains(vk::QueueFlags::GRAPHICS));

            for (present_queue, _) in queue_families.iter().enumerate() {
                let supports_surface = surface_fn.get_physical_device_surface_support(
                    physical_device,
                    present_queue as _,
                    surface,
                )?;
                if supports_surface {
                    return Ok(graphics_queue.map(|graphics_queue| {
                        (physical_device, graphics_queue as _, present_queue as _)
                    }));
                }
            }
        }
        Ok(None)
    }

    unsafe fn create_command_pool(
        device: &ash::Device,
        queue_family_index: u32,
    ) -> VkResult<Guarded<(vk::CommandPool, &ash::Device)>> {
        let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        Ok(device
            .create_command_pool(&command_pool_create_info, None)?
            .guard_with(device))
    }

    unsafe fn allocate_command_buffer(
        device: &ash::Device,
        command_pool: vk::CommandPool,
    ) -> VkResult<vk::CommandBuffer> {
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .command_buffer_count(1);
        Ok(device.allocate_command_buffers(&command_buffer_allocate_info)?[0])
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
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    resolution: vk::Extent2D,
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: Vec<vk::ImageView>,
    triangle_frag_shader_module: vk::ShaderModule,
    triangle_vert_shader_module: vk::ShaderModule,
}

#[derive(Error, Debug)]
pub enum SharedFrondError {
    #[error("Vulkan error occurred")]
    VkError(#[from] vk::Result), // TODO: split into contexts
    #[error("Couldn't select acceptable surface format")]
    NoAcceptableSurfaceFormat,
    #[error("Surface has no area")]
    NoSurfaceArea,
}

impl SharedFrond {
    pub fn new(stem: Arc<SharedStem>) -> Result<Self, SharedFrondError> {
        Self::new_with_swapchain(stem, vk::SwapchainKHR::null())
    }

    fn new_with_swapchain(
        stem: Arc<SharedStem>,
        old_swapchain: vk::SwapchainKHR,
    ) -> Result<Self, SharedFrondError> {
        let crown = stem.crown();
        let device = stem.device();

        let resolution = crown.window_resolution();
        if resolution.width == 0 || resolution.height == 0 {
            return Err(SharedFrondError::NoSurfaceArea);
        }

        unsafe {
            let surface_format = {
                let physical_device = stem.physical_device();

                let surface = crown.surface();
                let surface = surface.lock().unwrap();
                let surface_fn = crown.surface_fn();
                Self::select_surface_format(surface_fn, physical_device, *surface)?
                    .ok_or(SharedFrondError::NoAcceptableSurfaceFormat)?
            };

            let render_pass = Self::create_render_pass(device, surface_format.format)?;

            let triangle_vert_shader_module =
                Self::create_shader_module(device, include_glsl!("shaders/triangle.vert"))?;
            let triangle_frag_shader_module =
                Self::create_shader_module(device, include_glsl!("shaders/triangle.frag"))?;
            let pipeline_layout = device
                .create_pipeline_layout(&Default::default(), None)?
                .guard_with(device);
            let pipeline = Self::create_pipeline(
                device,
                *triangle_vert_shader_module,
                *triangle_frag_shader_module,
                resolution,
                *pipeline_layout,
                *render_pass,
            )?;

            let swapchain =
                Self::create_swapchain(&stem, surface_format, resolution, old_swapchain)?;

            let swapchain_image_views = Self::create_swapchain_image_views(
                stem.swapchain_fn(),
                device,
                *swapchain,
                surface_format.format,
            )?;

            let framebuffers = Self::create_framebuffers(
                device,
                *render_pass,
                &swapchain_image_views,
                resolution,
            )?;

            Ok(Self {
                pipeline: pipeline.take(),
                pipeline_layout: pipeline_layout.take(),
                render_pass: render_pass.take(),
                swapchain: swapchain.take(),
                swapchain_image_views: swapchain_image_views.take(),
                framebuffers: framebuffers.take(),
                triangle_frag_shader_module: triangle_frag_shader_module.take(),
                triangle_vert_shader_module: triangle_vert_shader_module.take(),
                resolution,
                stem,
            })
        }
    }

    unsafe fn select_surface_format(
        surface_fn: &Surface,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> VkResult<Option<vk::SurfaceFormatKHR>> {
        let surface_formats =
            surface_fn.get_physical_device_surface_formats(physical_device, surface)?;
        let desired_formats = [
            vk::SurfaceFormatKHR {
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
                format: vk::Format::B8G8R8A8_SRGB,
            },
            // TODO: Support other formats?
        ];
        Ok(desired_formats
            .iter()
            .find(|&&desired_format| surface_formats.iter().any(|&sfmt| sfmt == desired_format))
            .copied())
    }

    unsafe fn create_render_pass(
        device: &ash::Device,
        surface_format: vk::Format,
    ) -> VkResult<Guarded<(vk::RenderPass, &ash::Device)>> {
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
        Ok(device
            .create_render_pass(&render_pass_create_info, None)?
            .guard_with(device))
    }

    unsafe fn create_shader_module<'a>(
        device: &'a ash::Device,
        spirv: &[u32],
    ) -> VkResult<Guarded<(vk::ShaderModule, &'a ash::Device)>> {
        let shader_module_create_info = vk::ShaderModuleCreateInfo::builder().code(spirv);
        let shader_module = device.create_shader_module(&shader_module_create_info, None)?;
        Ok(shader_module.guard_with(device))
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

    unsafe fn create_swapchain(
        stem: &SharedStem,
        surface_format: vk::SurfaceFormatKHR,
        default_resolution: vk::Extent2D,
        old_swapchain: vk::SwapchainKHR,
    ) -> VkResult<Guarded<(vk::SwapchainKHR, &Swapchain)>> {
        let crown = stem.crown();
        let physical_device = stem.physical_device();
        let queues = stem.queues();
        let surface_fn = crown.surface_fn();
        let swapchain_fn = stem.swapchain_fn();
        let surface = crown.surface().lock().unwrap();

        let surface_capabilities =
            surface_fn.get_physical_device_surface_capabilities(physical_device, *surface)?;

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
            .get_physical_device_surface_present_modes(physical_device, *surface)?
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
            .surface(*surface)
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

        Ok(swapchain_fn
            .create_swapchain(&swapchain_create_info, None)?
            .guard_with(swapchain_fn))
    }

    unsafe fn create_swapchain_image_views<'a>(
        swapchain_fn: &Swapchain,
        device: &'a ash::Device,
        swapchain: vk::SwapchainKHR,
        format: vk::Format,
    ) -> VkResult<Guarded<(Vec<vk::ImageView>, &'a ash::Device)>> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1)
            .build();

        let mut image_views = Vec::<vk::ImageView>::new().guard_with(device);
        for image in swapchain_fn.get_swapchain_images(swapchain)? {
            let image_view_create_info = vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(subresource_range)
                .image(image);
            image_views.push(device.create_image_view(&image_view_create_info, None)?);
        }

        Ok(image_views)
    }

    unsafe fn create_framebuffers<'a>(
        device: &'a ash::Device,
        render_pass: vk::RenderPass,
        image_views: &[vk::ImageView],
        resolution: vk::Extent2D,
    ) -> VkResult<Guarded<(Vec<vk::Framebuffer>, &'a ash::Device)>> {
        let mut framebuffers = Vec::<vk::Framebuffer>::new().guard_with(device);
        for &image_view in image_views {
            let attachments = [image_view];
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

    pub fn take_swapchain(mut self) -> SharedFrondSwapchain {
        SharedFrondSwapchain {
            stem: self.stem.clone(),
            swapchain: std::mem::take(&mut self.swapchain),
        }
    }

    pub fn framebuffers(&self) -> &[vk::Framebuffer] {
        &self.framebuffers
    }

    pub fn pipeline(&self) -> vk::Pipeline {
        self.pipeline
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
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_shader_module(self.triangle_frag_shader_module, None);
            device.destroy_shader_module(self.triangle_vert_shader_module, None);
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

pub struct SharedFrondSwapchain {
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
}

impl SharedFrondSwapchain {
    pub fn ressurect(self) -> Result<SharedFrond, (SharedFrondSwapchain, SharedFrondError)> {
        SharedFrond::new_with_swapchain(self.stem.clone(), self.swapchain)
            .map_err(|err| (self, err))
    }

    pub fn stem(&self) -> Arc<SharedStem> {
        self.stem.clone()
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

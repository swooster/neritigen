use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

use ash::{
    extensions::{ext::DebugUtils, khr::Surface, khr::Swapchain},
    prelude::VkResult,
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::{self, Handle},
};
use crevice::std140::AsStd140;
use mint::ColumnMatrix4;
use thiserror::Error;
use vk_shader_macros::include_glsl;
use winit::window::Window;

use crate::{
    guard::{GuardableResource, Guarded},
    image::Image,
    util,
};

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

    pub unsafe fn set_name<T: Handle>(
        &self,
        device: &ash::Device,
        object: T,
        name: &str,
    ) -> VkResult<()> {
        let name = CString::new(name).unwrap();
        let name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
            .object_type(T::TYPE)
            .object_handle(object.as_raw())
            .object_name(&name);
        self.debug_utils_fn
            .debug_utils_set_object_name(device.handle(), &name_info)
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
    fullscreen_vert_shader_module: vk::ShaderModule,
    image_acquired_semaphore: vk::Semaphore,
    physical_device: vk::PhysicalDevice,
    physical_device_memory_properties: vk::PhysicalDeviceMemoryProperties,
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
    #[error("Couldn't select acceptable memory type for {0:?} and {1:?}")]
    NoAcceptableMeoryType(vk::MemoryRequirements, vk::MemoryPropertyFlags),
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

            drop(surface);

            let command_pool = Self::create_command_pool(&device, queues.graphics_family)?;
            crown.set_name(&device, *command_pool, "stem primary")?;
            let command_buffer = Self::allocate_command_buffer(&device, *command_pool)?;
            crown.set_name(&device, *command_pool, "stem primary")?;

            let image_acquired_semaphore = device
                .create_semaphore(&Default::default(), None)?
                .guard_with(&*device);
            crown.set_name(&device, *image_acquired_semaphore, "image acquired")?;
            let render_complete_semaphore = device
                .create_semaphore(&Default::default(), None)?
                .guard_with(&*device);
            crown.set_name(&device, *render_complete_semaphore, "render complete")?;

            let signaled_fence_create_info =
                vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            let presentation_fence = device
                .create_fence(&signaled_fence_create_info, None)?
                .guard_with(&*device);
            crown.set_name(&device, *presentation_fence, "presentation")?;

            let physical_device_memory_properties =
                instance.get_physical_device_memory_properties(physical_device);

            let fullscreen_vert_shader_module =
                util::create_shader_module(&device, include_glsl!("shaders/fullscreen.vert"))?;
            crown.set_name(&device, *fullscreen_vert_shader_module, "fullscreen vert")?;

            Ok(Self {
                command_pool: command_pool.take(),
                fullscreen_vert_shader_module: fullscreen_vert_shader_module.take(),
                image_acquired_semaphore: image_acquired_semaphore.take(),
                presentation_fence: presentation_fence.take(),
                render_complete_semaphore: render_complete_semaphore.take(),
                device: device.take(),
                command_buffer,
                crown,
                physical_device,
                physical_device_memory_properties,
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

    pub fn assert_is(&self, other: &Self) {
        if self as *const Self != other as *const Self {
            panic!("Mismatched stems");
        }
    }

    pub fn select_memory_type(
        &self,
        memory_requirements: vk::MemoryRequirements,
        required_flags: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        util::select_memory_type(
            self.physical_device_memory_properties,
            memory_requirements,
            required_flags,
        )
    }

    pub unsafe fn set_name<T: Handle>(&self, object: T, name: &str) -> VkResult<()> {
        self.crown.set_name(&self.device, object, name)
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

    pub fn fullscreen_vert_shader_module(&self) -> vk::ShaderModule {
        self.fullscreen_vert_shader_module
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
            let device = &self.device;
            let _ = device.device_wait_idle();

            device.destroy_shader_module(self.fullscreen_vert_shader_module, None);
            device.destroy_fence(self.presentation_fence, None);
            device.destroy_semaphore(self.image_acquired_semaphore, None);
            device.destroy_semaphore(self.render_complete_semaphore, None);
            device.destroy_command_pool(self.command_pool, None);
            device.destroy_device(None);
        }
    }
}

pub struct Queues {
    pub graphics: vk::Queue,
    pub graphics_family: u32,
    pub present: vk::Queue,
    pub present_family: u32,
}

#[derive(AsStd140)]
pub struct ViewBuffer {
    pub view: ColumnMatrix4<f32>,
}

impl ViewBuffer {
    pub fn push_constant_range() -> vk::PushConstantRange {
        vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: Self::std140_size_static() as _,
        }
    }
}

pub struct SharedFrond {
    depth_stencil: Image,
    diffuse: Image,
    light: Image,
    normal: Image,
    resolution: vk::Extent2D,
    shadow: Image,
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_format: vk::Format,
}

#[derive(Error, Debug)]
pub enum SharedFrondError {
    #[error("Vulkan error occurred")]
    VkError(#[from] vk::Result), // TODO: split into contexts
    #[error("Couldn't select acceptable surface format")]
    NoAcceptableSurfaceFormat,
    #[error("Couldn't select acceptable memory type for {0:?} and {1:?}")]
    NoAcceptableMeoryType(vk::MemoryRequirements, vk::MemoryPropertyFlags),
    #[error("Surface has no area")]
    NoSurfaceArea,
}

impl SharedFrond {
    pub fn new(stem: Arc<SharedStem>) -> Result<Self, SharedFrondError> {
        unsafe {
            let mut swapchain = vk::SwapchainKHR::null().guard_with(stem.swapchain_fn());
            Self::new_with_swapchain(stem.clone(), &mut swapchain)
        }
    }

    fn new_with_swapchain(
        stem: Arc<SharedStem>,
        // Icky, but easier that map_err() for every fallible call, while ensuring that
        // SharedFrondSwapchain::ressurect() always ends up with a valid swapchain on failure.
        swapchain: &mut vk::SwapchainKHR,
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

            *swapchain = Self::create_swapchain(&stem, surface_format, resolution, *swapchain)?;
            for image in stem.swapchain_fn().get_swapchain_images(*swapchain)? {
                stem.set_name(image, "presentation")?;
            }

            let swapchain_image_views = Self::create_swapchain_image_views(
                stem.swapchain_fn(),
                device,
                *swapchain,
                surface_format.format,
            )?;
            for image_view in swapchain_image_views.iter() {
                stem.set_name(*image_view, "presentation")?;
            }

            let diffuse = Self::create_image(
                &stem,
                resolution,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                "diffuse",
            )?;

            let normal = Self::create_image(
                &stem,
                resolution,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                "normal",
            )?;

            let depth_stencil = Self::create_image(
                &stem,
                resolution,
                vk::Format::D24_UNORM_S8_UINT,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                "depth_stencil",
            )?;

            let shadow = Self::create_image(
                &stem,
                vk::Extent2D {
                    width: 1024,
                    height: 1024,
                },
                vk::Format::D24_UNORM_S8_UINT,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::DEPTH,
                "shadow",
            )?;

            let light = Self::create_image(
                &stem,
                resolution,
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
                "light",
            )?;

            Ok(Self {
                depth_stencil: depth_stencil.take(),
                diffuse: diffuse.take(),
                light: light.take(),
                normal: normal.take(),
                shadow: shadow.take(),
                swapchain: std::mem::take(swapchain),
                swapchain_image_views: swapchain_image_views.take(),
                resolution,
                stem,
                swapchain_format: surface_format.format,
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

    unsafe fn create_swapchain(
        stem: &SharedStem,
        surface_format: vk::SurfaceFormatKHR,
        default_resolution: vk::Extent2D,
        old_swapchain: vk::SwapchainKHR,
    ) -> VkResult<vk::SwapchainKHR> {
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

        swapchain_fn.create_swapchain(&swapchain_create_info, None)
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

    unsafe fn create_image<'a>(
        stem: &'a SharedStem,
        resolution: vk::Extent2D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspects: vk::ImageAspectFlags,
        name: &str,
    ) -> Result<Guarded<(Image, &'a ash::Device)>, SharedFrondError> {
        let select_device_local_memory = |memory_requirements: vk::MemoryRequirements| {
            stem.select_memory_type(memory_requirements, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                .ok_or(SharedFrondError::NoAcceptableMeoryType(
                    memory_requirements,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ))
        };

        let queue_family_indices = [stem.queues().graphics_family];

        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: resolution.width,
                height: resolution.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .queue_family_indices(&queue_family_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = Image::new(
            stem.device(),
            &image_create_info,
            select_device_local_memory,
            aspects,
        )??;

        stem.set_name(image.image, name)?;
        stem.set_name(image.memory, name)?;
        stem.set_name(image.view, name)?;

        Ok(image)
    }

    pub fn take_swapchain(mut self) -> SharedFrondSwapchain {
        SharedFrondSwapchain {
            stem: self.stem.clone(),
            swapchain: std::mem::take(&mut self.swapchain),
        }
    }

    pub fn needs_resizing(&self) -> bool {
        self.resolution() != self.stem().crown().window_resolution()
    }

    pub fn depth_stencil(&self) -> &Image {
        &self.depth_stencil
    }

    pub fn device(&self) -> &ash::Device {
        self.stem.device()
    }

    pub fn diffuse(&self) -> &Image {
        &self.diffuse
    }

    pub fn light(&self) -> &Image {
        &self.light
    }

    pub fn normal(&self) -> &Image {
        &self.normal
    }

    pub fn resolution(&self) -> vk::Extent2D {
        self.resolution
    }

    pub fn shadow(&self) -> &Image {
        &self.shadow
    }

    pub fn stem(&self) -> Arc<SharedStem> {
        self.stem.clone()
    }

    pub fn swapchain(&self) -> vk::SwapchainKHR {
        self.swapchain
    }

    pub fn swapchain_format(&self) -> vk::Format {
        self.swapchain_format
    }

    pub fn swapchain_image_views(&self) -> &[vk::ImageView] {
        &self.swapchain_image_views
    }
}

impl Drop for SharedFrond {
    fn drop(&mut self) {
        let device = self.stem.device();
        let swapchain_fn = self.stem.swapchain_fn();
        unsafe {
            let _ = device.device_wait_idle();

            self.shadow.destroy_with(device);
            self.normal.destroy_with(device);
            self.light.destroy_with(device);
            self.diffuse.destroy_with(device);
            self.depth_stencil.destroy_with(device);
            for &image_view in self.swapchain_image_views.iter() {
                device.destroy_image_view(image_view, None);
            }
            swapchain_fn.destroy_swapchain(self.swapchain, None);
        }
    }
}

pub struct SharedFrondSwapchain {
    stem: Arc<SharedStem>,
    swapchain: vk::SwapchainKHR,
}

impl SharedFrondSwapchain {
    pub fn resurrect(mut self) -> Result<SharedFrond, (SharedFrondSwapchain, SharedFrondError)> {
        SharedFrond::new_with_swapchain(self.stem.clone(), &mut self.swapchain)
            .map_err(|err| (self, err))
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

use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::Arc;

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
    surface: vk::SurfaceKHR,
    surface_fn: Surface,
    _window: Arc<Window>,
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

            let surface = ash_window::create_surface(&entry, &instance, &*window, None).unwrap();
            let surface_fn = Surface::new(&entry, &instance);

            Self {
                debug_utils_fn,
                debug_utils_messenger,
                _entry: entry,
                instance,
                surface,
                surface_fn,
                _window: window,
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

    pub fn surface(&self) -> vk::SurfaceKHR {
        self.surface
    }

    pub fn surface_fn(&self) -> &Surface {
        &self.surface_fn
    }
}

impl Drop for SharedCrown {
    fn drop(&mut self) {
        unsafe {
            self.surface_fn.destroy_surface(self.surface, None);
            self.debug_utils_fn
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub struct SharedStem {
    command_buffer: vk::CommandBuffer,
    command_pool: vk::CommandPool,
    crown: SharedCrown,
    device: ash::Device,
    image_acquired_semaphore: vk::Semaphore,
    physical_device: vk::PhysicalDevice,
    presentation_fence: vk::Fence,
    queues: Queues,
    render_complete_semaphore: vk::Semaphore,
    swapchain_fn: Swapchain,
}

impl SharedStem {
    pub fn new(crown: SharedCrown) -> Self {
        let instance = crown.instance();
        let surface = crown.surface();
        let surface_fn = crown.surface_fn();

        unsafe {
            let (physical_device, device, queues) =
                Self::create_device_and_queues(instance, surface_fn, surface);

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

    pub fn surface(&self) -> vk::SurfaceKHR {
        self.crown.surface()
    }

    pub fn surface_fn(&self) -> &Surface {
        self.crown.surface_fn()
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

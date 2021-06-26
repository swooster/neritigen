use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::Arc;

use ash::{
    extensions::{ext::DebugUtils, khr::Surface},
    version::{EntryV1_0, InstanceV1_0},
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

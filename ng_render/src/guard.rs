use std::ops::{Deref, DerefMut};

use ash::{
    extensions::{ext, khr},
    version::{DeviceV1_0, InstanceV1_0},
    vk,
};
use scopeguard::ScopeGuard;

pub trait GuardableResource
where
    Self: Sized,
{
    unsafe fn guard(self) -> Guarded<Self>
    where
        Self: Guardable;

    unsafe fn guard_with<C>(self, context: C) -> Guarded<(Self, C)>
    where
        (Self, C): Guardable;
}

impl<R> GuardableResource for R {
    unsafe fn guard(self) -> Guarded<Self>
    where
        Self: Guardable,
    {
        Guarded::new(self)
    }

    unsafe fn guard_with<C>(self, context: C) -> Guarded<(Self, C)>
    where
        (Self, C): Guardable,
    {
        Guarded::new((self, context))
    }
}

pub struct Guarded<T>(ScopeGuard<T, fn(T)>);

impl<T: Guardable> Guarded<T> {
    pub unsafe fn new(value: T) -> Self {
        fn drop<T: Guardable>(value: T) {
            unsafe { <T as Guardable>::drop(value) }
        }
        Self(ScopeGuard::with_strategy(value, drop))
    }

    pub fn take(self) -> T::Resource {
        ScopeGuard::into_inner(self.0).take()
    }
}

impl<T: Guardable> Deref for Guarded<T> {
    type Target = T::Resource;

    fn deref(&self) -> &Self::Target {
        <T as Guardable>::deref(&self.0)
    }
}

impl<T: Guardable> DerefMut for Guarded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        <T as Guardable>::deref_mut(&mut self.0)
    }
}

pub trait Guardable {
    type Resource;

    fn deref(&self) -> &Self::Resource;

    fn deref_mut(&mut self) -> &mut Self::Resource;

    fn take(self) -> Self::Resource;

    unsafe fn drop(self);
}

impl<R, C> Guardable for (Vec<R>, C)
where
    (R, C): Guardable,
    C: Clone,
{
    type Resource = Vec<R>;

    fn deref(&self) -> &Self::Resource {
        &self.0
    }

    fn deref_mut(&mut self) -> &mut Self::Resource {
        &mut self.0
    }

    fn take(self) -> Self::Resource {
        self.0
    }

    unsafe fn drop(self) {
        let (resources, context) = self;
        for resource in resources {
            (resource, context.clone()).drop();
        }
    }
}

macro_rules! define_guardable {
    ($Resource:ty, $destroy:ident) => {
        impl Guardable for $Resource {
            type Resource = $Resource;

            fn deref(&self) -> &Self::Resource {
                self
            }

            fn deref_mut(&mut self) -> &mut Self::Resource {
                self
            }

            fn take(self) -> Self::Resource {
                self
            }

            unsafe fn drop(self) {
                self.$destroy(None);
            }
        }
    };

    ($Resource:ty, $Context:ty, $destroy:ident) => {
        impl<C> Guardable for ($Resource, C)
        where
            C: Deref<Target = $Context>,
        {
            type Resource = $Resource;

            fn deref(&self) -> &Self::Resource {
                &self.0
            }

            fn deref_mut(&mut self) -> &mut Self::Resource {
                &mut self.0
            }

            fn take(self) -> Self::Resource {
                self.0
            }

            unsafe fn drop(self) {
                let (resource, context) = self;
                context.$destroy(resource, None);
            }
        }
    };
}

define_guardable!(ash::Instance, destroy_instance);
define_guardable!(ash::Device, destroy_device);
define_guardable!(
    vk::DebugUtilsMessengerEXT,
    ext::DebugUtils,
    destroy_debug_utils_messenger
);
define_guardable!(vk::SurfaceKHR, khr::Surface, destroy_surface);
define_guardable!(vk::SwapchainKHR, khr::Swapchain, destroy_swapchain);
define_guardable!(vk::CommandPool, ash::Device, destroy_command_pool);
define_guardable!(vk::DescriptorPool, ash::Device, destroy_descriptor_pool);
define_guardable!(vk::DescriptorSetLayout, ash::Device, destroy_descriptor_set_layout);
define_guardable!(vk::DeviceMemory, ash::Device, free_memory);
define_guardable!(vk::Fence, ash::Device, destroy_fence);
define_guardable!(vk::Framebuffer, ash::Device, destroy_framebuffer);
define_guardable!(vk::Image, ash::Device, destroy_image);
define_guardable!(vk::ImageView, ash::Device, destroy_image_view);
define_guardable!(vk::Pipeline, ash::Device, destroy_pipeline);
define_guardable!(vk::PipelineLayout, ash::Device, destroy_pipeline_layout);
define_guardable!(vk::RenderPass, ash::Device, destroy_render_pass);
define_guardable!(vk::Semaphore, ash::Device, destroy_semaphore);
define_guardable!(vk::ShaderModule, ash::Device, destroy_shader_module);

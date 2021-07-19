use std::ops::Deref;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use crate::guard::{Guardable, GuardableResource, Guarded};

pub struct Image {
    pub format: vk::Format,
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub resolution: vk::Extent3D,
    pub view: vk::ImageView,
}

impl Image {
    pub unsafe fn new<D, E>(
        device: D,
        image_create_info: &vk::ImageCreateInfo,
        select_memory_type: impl Fn(vk::MemoryRequirements) -> Result<u32, E>,
        aspects: vk::ImageAspectFlags,
    ) -> VkResult<Result<Guarded<(Self, D)>, E>>
    where
        D: Deref<Target = ash::Device> + Clone,
    {
        let image = device
            .create_image(&image_create_info, None)?
            .guard_with(device.clone());

        let image_memory_requirements = device.get_image_memory_requirements(*image);
        let memory_type = match select_memory_type(image_memory_requirements) {
            Ok(memory_type) => memory_type,
            Err(err) => return Ok(Err(err)),
        };

        let allocate_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(image_memory_requirements.size)
            .memory_type_index(memory_type);
        let memory = device
            .allocate_memory(&allocate_info, None)?
            .guard_with(device.clone());

        device.bind_image_memory(*image, *memory, 0)?;

        let view_type = match image_create_info.image_type {
            vk::ImageType::TYPE_1D => vk::ImageViewType::TYPE_1D,
            vk::ImageType::TYPE_2D => vk::ImageViewType::TYPE_2D,
            vk::ImageType::TYPE_3D => vk::ImageViewType::TYPE_3D,
            other => panic!("Unknown vk::ImageType: {:?}", other),
        };
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(aspects)
            .level_count(1)
            .layer_count(1);
        let image_view_create_info = vk::ImageViewCreateInfo::builder()
            .image(*image)
            .view_type(view_type)
            .format(image_create_info.format)
            .subresource_range(subresource_range.build());
        let view = device
            .create_image_view(&image_view_create_info, None)?
            .guard_with(device.clone());

        let image = Self {
            format: image_create_info.format,
            image: image.take(),
            memory: memory.take(),
            resolution: image_create_info.extent,
            view: view.take(),
        };
        Ok(Ok(image.guard_with(device)))
    }

    pub fn resolution_2d(&self) -> vk::Extent2D {
        assert_eq!(self.resolution.depth, 1);
        vk::Extent2D {
            width: self.resolution.width,
            height: self.resolution.height,
        }
    }

    pub unsafe fn destroy_with(&mut self, device: &ash::Device) {
        device.destroy_image_view(self.view, None);
        device.destroy_image(self.image, None);
        device.free_memory(self.memory, None);
    }
}

impl<C> Guardable for (Image, C)
where
    C: Deref<Target = ash::Device>,
{
    type Resource = Image;

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
        let (mut resource, context) = self;
        resource.destroy_with(&*context);
    }
}

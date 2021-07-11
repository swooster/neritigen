use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use crate::guard::{GuardableResource, Guarded};

pub unsafe fn create_descriptor_pool<'a>(
    device: &'a ash::Device,
    max_sets: u32,
    pool_sizes: &[vk::DescriptorPoolSize],
) -> VkResult<Guarded<(vk::DescriptorPool, &'a ash::Device)>> {
    let descriptor_pool_create_info = vk::DescriptorPoolCreateInfo::builder()
        .max_sets(max_sets)
        .pool_sizes(pool_sizes);
    Ok(device
        .create_descriptor_pool(&descriptor_pool_create_info, None)?
        .guard_with(device))
}

pub unsafe fn create_shader_module<'a>(
    device: &'a ash::Device,
    spirv: &[u32],
) -> VkResult<Guarded<(vk::ShaderModule, &'a ash::Device)>> {
    let shader_module_create_info = vk::ShaderModuleCreateInfo::builder().code(spirv);
    let shader_module = device.create_shader_module(&shader_module_create_info, None)?;
    Ok(shader_module.guard_with(device))
}

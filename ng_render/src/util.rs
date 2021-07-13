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

pub unsafe fn create_pipeline_layout<'a>(
    device: &'a ash::Device,
    set_layouts: &[vk::DescriptorSetLayout],
    push_constant_ranges: &[vk::PushConstantRange],
) -> VkResult<Guarded<(vk::PipelineLayout, &'a ash::Device)>> {
    let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::builder()
        .set_layouts(set_layouts)
        .push_constant_ranges(push_constant_ranges);
    Ok(device
        .create_pipeline_layout(&pipeline_layout_create_info, None)?
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

pub fn select_memory_type(
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    memory_requirements: vk::MemoryRequirements,
    required_flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_properties.memory_types[..memory_properties.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_requirements.memory_type_bits != 0
                && memory_type.property_flags & required_flags == required_flags
        })
        .map(|(index, _)| index as _)
}
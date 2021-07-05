use std::sync::Arc;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use thiserror::Error;
use winit::window::Window;

use crate::shared;

use shared::{
    SharedCrown, SharedCrownError, SharedFrond, SharedFrondError, SharedFrondSwapchain, SharedStem,
    SharedStemError,
};

pub enum Renderer {
    Ready(SharedFrond),
    NeedsResize(SharedFrondSwapchain),
    NeedsDevice(Arc<SharedCrown>),
}

#[derive(Error, Debug)]
pub enum RendererError {
    #[error("Vulkan error occurred")]
    VkError(#[from] vk::Result), // TODO: split into contexts
    #[error("Unable to create renderer crown")]
    CrownCreationError(#[from] SharedCrownError),
    #[error("Unable to create renderer stem")]
    StemCreationError(#[from] SharedStemError),
    #[error("Unable to create renderer frond")]
    FrondCreationError(#[from] SharedFrondError),
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let crown = SharedCrown::new(window)?;
        Ok(Self::NeedsDevice(Arc::new(crown)))
    }

    fn crown(&mut self) -> Arc<SharedCrown> {
        match self {
            Self::Ready(frond) => frond.stem().crown(),
            Self::NeedsResize(swapchain) => swapchain.stem().crown(),
            Self::NeedsDevice(crown) => crown.clone(),
        }
    }

    fn take(&mut self) -> Self {
        let crown = self.crown();
        std::mem::replace(self, Self::NeedsDevice(crown))
    }

    fn resize(&mut self) {
        *self = match self.take() {
            Self::Ready(frond)
                if frond.stem().crown().window_resolution() != frond.resolution() =>
            {
                Self::NeedsResize(frond.take_swapchain())
            }
            x => x,
        };
    }

    fn rebuild(&mut self) -> Result<&mut SharedFrond, RendererError> {
        self.resize();

        let frond = match self.take() {
            Self::NeedsDevice(crown) => {
                let stem = SharedStem::new(crown)?;
                SharedFrond::new(Arc::new(stem))?
            }
            Self::NeedsResize(swapchain) => match swapchain.ressurect() {
                Ok(frond) => frond,
                Err((swapchain, err)) => {
                    *self = Self::NeedsResize(swapchain);
                    return Err(err.into());
                }
            },
            Self::Ready(frond) => frond,
        };

        *self = Self::Ready(frond);
        if let Self::Ready(ref mut frond) = self {
            Ok(frond)
        } else {
            unreachable!()
        }
    }

    // Returns Ok(successfully drew)
    pub fn draw(&mut self) -> Result<bool, RendererError> {
        fn inner(frond: &SharedFrond) -> VkResult<bool> {
            let framebuffers = frond.framebuffers();
            let render_pass = frond.render_pass();
            let resolution = frond.resolution();
            let swapchain = frond.swapchain();

            let stem = frond.stem();
            let command_buffer = stem.command_buffer();
            let device = stem.device();
            let image_acquired_semaphore = stem.image_acquired_semaphore();
            let presentation_fence = stem.presentation_fence();
            let queues = stem.queues();
            let render_complete_semaphore = stem.render_complete_semaphore();
            let swapchain_fn = stem.swapchain_fn();

            unsafe {
                device.wait_for_fences(&[presentation_fence], true, u64::MAX)?;
                device.reset_fences(&[presentation_fence])?;

                let (image_index, suboptimal_acquire) = swapchain_fn.acquire_next_image(
                    swapchain,
                    u64::MAX,
                    image_acquired_semaphore,
                    vk::Fence::null(),
                )?;

                let command_buffer = command_buffer;
                device.reset_command_buffer(
                    command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )?;
                let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                device.begin_command_buffer(command_buffer, &command_buffer_begin_info)?;

                let clear_values = [vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                }];

                let render_area = vk::Rect2D {
                    offset: Default::default(),
                    extent: resolution,
                };

                let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                    .render_pass(render_pass)
                    .framebuffer(framebuffers[image_index as usize])
                    .render_area(render_area)
                    .clear_values(&clear_values);
                device.cmd_begin_render_pass(
                    command_buffer,
                    &render_pass_begin_info,
                    vk::SubpassContents::INLINE,
                );

                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    frond.pipeline(),
                );

                device.cmd_draw(
                    command_buffer,
                    3, // vertices
                    1, // instances
                    0, // first vertex
                    0, // first instance
                );

                device.cmd_end_render_pass(command_buffer);

                device.end_command_buffer(command_buffer)?;

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
                device.queue_submit(queues.graphics, &submit_infos, presentation_fence)?;

                let wait_semaphores = [render_complete_semaphore];
                let swapchains = [swapchain];
                let image_indices = [image_index];
                let present_info = vk::PresentInfoKHR::builder()
                    .wait_semaphores(&wait_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);
                let suboptimal_present =
                    swapchain_fn.queue_present(queues.present, &present_info)?;

                Ok(!suboptimal_acquire && !suboptimal_present)
            }
        }

        let frond = match self.rebuild() {
            Err(RendererError::FrondCreationError(SharedFrondError::NoSurfaceArea)) => {
                return Ok(false)
            }
            x => x,
        }?;

        let result = inner(frond);
        if result == Err(vk::Result::ERROR_DEVICE_LOST) {
            self.take();
        }
        Ok(result?)
    }
}

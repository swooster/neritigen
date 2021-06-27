use std::sync::Arc;

use ash::{version::DeviceV1_0, vk};
use winit::window::Window;

mod shared;

use shared::{SharedCrown, SharedFrond, SharedFrondSwapchain, SharedStem};

pub enum Renderer {
    Live(SharedFrond),
    Dead(SharedFrondSwapchain),
    Invalid, // FIXME: use something like replace_with() to avoid this?
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        let shared_crown = Arc::new(SharedCrown::new(window));
        let shared_stem = Arc::new(SharedStem::new(shared_crown));
        let shared_frond = SharedFrond::new(shared_stem);

        Self::Live(shared_frond)
    }

    pub fn draw(&mut self) {
        if let Self::Live(ref frond) = self {
            if frond.stem().crown().window_resolution() != frond.resolution() {
                self.resize();
            }
        }
        self.ressurect();

        if let Self::Live(ref frond) = self {
            Self::_draw(frond);
        }
    }

    fn _draw(frond: &SharedFrond) -> bool {
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
            device
                .wait_for_fences(&[presentation_fence], true, u64::MAX)
                .unwrap();
            device.reset_fences(&[presentation_fence]).unwrap();

            let (image_index, suboptimal_acquire) = swapchain_fn
                .acquire_next_image(
                    swapchain,
                    u64::MAX,
                    image_acquired_semaphore,
                    vk::Fence::null(),
                )
                .unwrap();
            assert!(!suboptimal_acquire);

            let command_buffer = command_buffer;
            device
                .reset_command_buffer(
                    command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .unwrap();
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                .unwrap();

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 1.0, 0.25, 1.0],
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

            device.cmd_end_render_pass(command_buffer);

            device.end_command_buffer(command_buffer).unwrap();

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
            device
                .queue_submit(queues.graphics, &submit_infos, presentation_fence)
                .unwrap();

            let wait_semaphores = [render_complete_semaphore];
            let swapchains = [swapchain];
            let image_indices = [image_index];
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(&wait_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_result = swapchain_fn.queue_present(queues.present, &present_info);
            present_result == Ok(false)
        }
    }

    fn resize(&mut self) {
        *self = match std::mem::replace(self, Self::Invalid) {
            Self::Live(frond) => Self::Dead(frond.take_swapchain()),
            x => x,
        };
    }

    fn ressurect(&mut self) {
        *self = match std::mem::replace(self, Self::Invalid) {
            Self::Dead(swapchain) => match swapchain.ressurect() {
                Ok(frond) => Self::Live(frond),
                Err(swapchain) => Self::Dead(swapchain),
            },
            x => x,
        };
    }
}

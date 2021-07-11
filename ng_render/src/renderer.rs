use std::sync::Arc;

use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use thiserror::Error;
use winit::window::Window;

use crate::{
    geometry::{GeometryFrond, GeometryStem},
    lighting::{LightingFrond, LightingStem},
    shared::{
        SharedCrown, SharedCrownError, SharedFrond, SharedFrondError, SharedFrondSwapchain,
        SharedStem, SharedStemError,
    },
    tonemapping::{TonemappingFrond, TonemappingStem},
};

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

pub struct Renderer {
    crown: RendererCrown,
    stem_and_frond: Option<RendererStemAndFrond>,
}

struct RendererStemAndFrond {
    stem: RendererStem,
    frond: Result<RendererFrond, SharedFrondSwapchain>,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        Ok(Self {
            crown: RendererCrown::new(window)?,
            stem_and_frond: None,
        })
    }

    fn rebuild(&mut self) -> Result<&mut RendererFrond, RendererError> {
        let (stem, frond) = match self.stem_and_frond.take() {
            Some(RendererStemAndFrond { stem, frond }) => (stem, frond),
            None => {
                let stem = RendererStem::new(&self.crown)?;
                let frond = Ok(RendererFrond::new(&stem)?);
                (stem, frond)
            }
        };

        let frond = match frond {
            Ok(frond) if frond.shared.needs_resizing() => Err(frond.take_swapchain()),
            x => x,
        };

        let (frond, err) =
            match frond.or_else(|swapchain| RendererFrond::resurrect(&stem, swapchain)) {
                Ok(frond) => (Ok(frond), Ok(())),
                Err((swapchain, err)) => (Err(swapchain), Err(err)),
            };
        let stem_and_frond = self
            .stem_and_frond
            .insert(RendererStemAndFrond { stem, frond });
        err?;
        match &mut stem_and_frond.frond {
            Ok(frond) => Ok(frond),
            _ => unreachable!(),
        }
    }

    fn lose_device(&mut self) {
        self.stem_and_frond = None;
    }

    pub fn draw(&mut self) -> Result<bool, RendererError> {
        let frond = match self.rebuild() {
            Err(RendererError::FrondCreationError(SharedFrondError::NoSurfaceArea)) => {
                return Ok(false)
            }
            x => x,
        }?;

        let result = unsafe { frond.draw() };
        if result == Err(vk::Result::ERROR_DEVICE_LOST) {
            self.lose_device();
        }
        Ok(result?)
    }
}

struct RendererCrown {
    shared: Arc<SharedCrown>,
}

impl RendererCrown {
    pub fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let shared = Arc::new(SharedCrown::new(window)?);
        Ok(Self { shared })
    }
}

struct RendererStem {
    geometry: Arc<GeometryStem>,
    lighting: Arc<LightingStem>,
    shared: Arc<SharedStem>,
    tonemapping: Arc<TonemappingStem>,
}

impl RendererStem {
    fn new(crown: &RendererCrown) -> Result<Self, RendererError> {
        let shared = Arc::new(SharedStem::new(crown.shared.clone())?);
        let geometry = Arc::new(GeometryStem::new(shared.clone())?);
        let lighting = Arc::new(LightingStem::new(shared.clone())?);
        let tonemapping = Arc::new(TonemappingStem::new(shared.clone())?);

        Ok(Self {
            geometry,
            lighting,
            shared,
            tonemapping,
        })
    }
}

struct RendererFrond {
    geometry: Arc<GeometryFrond>,
    lighting: Arc<LightingFrond>,
    shared: Arc<SharedFrond>,
    tonemapping: Arc<TonemappingFrond>,
}

impl RendererFrond {
    fn new(stem: &RendererStem) -> Result<Self, RendererError> {
        let shared = Arc::new(SharedFrond::new(stem.shared.clone())?);
        Self::new_from_shared_frond(stem, shared)
    }

    fn resurrect(
        stem: &RendererStem,
        swapchain: SharedFrondSwapchain,
    ) -> Result<Self, (SharedFrondSwapchain, RendererError)> {
        let shared = Arc::new(
            swapchain
                .resurrect()
                .map_err(|(swapchain, err)| (swapchain, err.into()))?,
        );

        Self::new_from_shared_frond(stem, shared.clone()).map_err(|err| {
            let swapchain = match Arc::try_unwrap(shared.clone()) {
                Ok(shared) => shared.take_swapchain(),
                _ => panic!(
                    "Cannot take swapchain from SharedFrond as something is holding onto it."
                ),
            };
            (swapchain, err)
        })
    }

    fn new_from_shared_frond(
        stem: &RendererStem,
        shared: Arc<SharedFrond>,
    ) -> Result<Self, RendererError> {
        let geometry = Arc::new(GeometryFrond::new(stem.geometry.clone(), shared.clone())?);
        let lighting = Arc::new(LightingFrond::new(stem.lighting.clone(), shared.clone())?);
        let tonemapping = Arc::new(TonemappingFrond::new(
            stem.tonemapping.clone(),
            shared.clone(),
        )?);

        Ok(Self {
            geometry,
            lighting,
            shared,
            tonemapping,
        })
    }

    unsafe fn draw(&self) -> VkResult<bool> {
        let frond = &self.shared;

        let swapchain = frond.swapchain();

        let stem = frond.stem();
        let command_buffer = stem.command_buffer();
        let device = stem.device();
        let image_acquired_semaphore = stem.image_acquired_semaphore();
        let presentation_fence = stem.presentation_fence();
        let queues = stem.queues();
        let render_complete_semaphore = stem.render_complete_semaphore();
        let swapchain_fn = stem.swapchain_fn();

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

        self.geometry.draw(command_buffer);
        self.lighting.draw(command_buffer);
        self.tonemapping.draw(command_buffer, image_index);

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
        let suboptimal_present = swapchain_fn.queue_present(queues.present, &present_info)?;

        Ok(!suboptimal_acquire && !suboptimal_present)
    }

    fn take_swapchain(self) -> SharedFrondSwapchain {
        let Self {
            geometry,
            lighting,
            shared,
            tonemapping,
        } = self;
        drop((geometry, lighting, tonemapping));
        match Arc::try_unwrap(shared) {
            Ok(shared) => shared.take_swapchain(),
            _ => panic!("Cannot take swapchain from SharedFrond as something is holding onto it."),
        }
    }
}

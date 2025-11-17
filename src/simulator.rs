use std::{
    sync::{Arc, RwLock, mpsc::SyncSender},
    time::Instant,
};

use autd3_core::link::TxMessage;
use wgpu::InstanceFlags;
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::Window,
};

use crate::{
    emulator::EmulatorWrapper,
    error::Result,
    event::{EventResult, Signal, UserEvent},
    renderer::Renderer,
    server::Server,
    state::State,
    update_flag::UpdateFlag,
};

pub struct Simulator {
    server: Option<Server>,
    tx_buffer_queue: SyncSender<Vec<TxMessage>>,
    emulator: EmulatorWrapper,
    instance: wgpu::Instance,
    repaint_proxy: Option<EventLoopProxy<UserEvent>>,
    windows_next_repaint_time: Option<Instant>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    run_result: Result<()>,
    update_flag: UpdateFlag,
    state: State,
}

impl Simulator {
    pub fn run(event_loop: winit::event_loop::EventLoop<UserEvent>, state: State) -> Result<State> {
        let (buffer_queue_sender, buffer_queue_receiver) = std::sync::mpsc::sync_channel(16);

        let rx_buf = Arc::new(RwLock::default());
        let server = Server::new(
            state.port,
            rx_buf.clone(),
            buffer_queue_receiver,
            event_loop.create_proxy(),
        )?;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: if state.debug {
                InstanceFlags::VALIDATION | InstanceFlags::GPU_BASED_VALIDATION
            } else {
                InstanceFlags::empty()
            },
            ..Default::default()
        });

        let mut app = Self {
            instance,
            repaint_proxy: Some(event_loop.create_proxy()),
            tx_buffer_queue: buffer_queue_sender,
            server: Some(server),
            emulator: EmulatorWrapper::new(rx_buf),
            windows_next_repaint_time: None,
            window: None,
            renderer: None,
            run_result: Ok(()),
            update_flag: UpdateFlag::empty(),
            state,
        };

        event_loop.run_app(&mut app)?;

        app.run_result?;

        Ok(app.state)
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let egui_ctx = Renderer::create_egui_context();
        let window = self.create_window(&egui_ctx, event_loop)?;
        self.init_run_state(egui_ctx, window)?;
        Ok(())
    }

    fn create_window(
        &self,
        egui_ctx: &egui::Context,
        event_loop: &ActiveEventLoop,
    ) -> Result<Window> {
        let viewport_builder = egui::ViewportBuilder::default()
            .with_inner_size([self.state.window_size.0 as _, self.state.window_size.1 as _])
            .with_visible(false)
            .with_title("AUTD3 Simulator");
        let window = egui_winit::create_window(egui_ctx, event_loop, &viewport_builder)?;
        Ok(window)
    }

    fn init_run_state(&mut self, egui_ctx: egui::Context, window: Window) -> Result<()> {
        let window = Arc::new(window);

        self.renderer = Some(Renderer::new(
            &self.instance,
            self.repaint_proxy.take().unwrap(),
            egui_ctx,
            window.clone(),
            self.state.window_size.0,
            self.state.window_size.1,
            &self.state,
        )?);
        self.window = Some(window);

        Ok(())
    }

    fn update(&mut self, event: Option<Signal>) {
        let system_time = self.state.system_time();
        self.emulator.update(system_time);

        if let Some(signal) = event {
            match signal {
                crate::event::Signal::ConfigGeometry(geometry) => {
                    self.emulator.initialize(&geometry);
                    self.renderer.as_mut().unwrap().initialize(&self.emulator);

                    self.update_flag.set(UpdateFlag::UPDATE_CAMERA, true);
                    self.update_flag.set(UpdateFlag::UPDATE_TRANS_POS, true);
                    self.update_flag.set(UpdateFlag::UPDATE_TRANS_ALPHA, true);
                    self.update_flag.set(UpdateFlag::UPDATE_TRANS_STATE, true);
                    self.update_flag.set(UpdateFlag::UPDATE_SLICE_POS, true);
                    self.update_flag.set(UpdateFlag::UPDATE_SLICE_SIZE, true);
                    self.update_flag
                        .set(UpdateFlag::UPDATE_SLICE_COLOR_MAP, true);
                    self.update_flag.set(UpdateFlag::UPDATE_CONFIG, true);
                }
                crate::event::Signal::UpdateGeometry(geometry) => {
                    self.emulator.update_geometry(&geometry);

                    self.update_flag.set(UpdateFlag::UPDATE_TRANS_POS, true);
                }
                crate::event::Signal::Send(tx) => {
                    self.emulator.send(&tx);
                    self.tx_buffer_queue.send(tx).unwrap();

                    self.update_flag.set(UpdateFlag::UPDATE_TRANS_STATE, true);
                }
                crate::event::Signal::Close => {
                    self.emulator.clear();
                }
            }
        }
    }

    fn run_ui_and_paint(&mut self, window: &Window) -> Result<EventResult> {
        let Self {
            renderer,
            state,
            emulator,
            update_flag,
            ..
        } = self;

        if let Some(renderer) = renderer {
            if update_flag.contains(UpdateFlag::UPDATE_CAMERA) {
                renderer.update_camera(state, window);
                update_flag.remove(UpdateFlag::UPDATE_CAMERA);
            }

            if update_flag.contains(UpdateFlag::UPDATE_TRANS_POS) {
                renderer.update_trans_pos(emulator);
                update_flag.remove(UpdateFlag::UPDATE_TRANS_POS);
            }

            if update_flag.contains(UpdateFlag::UPDATE_TRANS_ALPHA)
                | update_flag.contains(UpdateFlag::UPDATE_TRANS_STATE)
            {
                if update_flag.contains(UpdateFlag::UPDATE_TRANS_STATE) {
                    emulator.update_transducers(state.mod_enable);
                    renderer.update_trans_state(emulator);

                    update_flag.remove(UpdateFlag::UPDATE_TRANS_STATE);
                }
                renderer.update_color(emulator);
                update_flag.remove(UpdateFlag::UPDATE_TRANS_ALPHA);
            }

            if update_flag.contains(UpdateFlag::UPDATE_SLICE_POS)
                | update_flag.contains(UpdateFlag::UPDATE_SLICE_SIZE)
            {
                renderer.update_slice(state);
                update_flag.remove(UpdateFlag::UPDATE_SLICE_POS);
                update_flag.remove(UpdateFlag::UPDATE_SLICE_SIZE);
            }

            if update_flag.contains(UpdateFlag::UPDATE_CONFIG) {
                renderer.update_config(state, emulator);
                update_flag.remove(UpdateFlag::UPDATE_CONFIG);
            }

            if update_flag.contains(UpdateFlag::UPDATE_SLICE_COLOR_MAP) {
                renderer.update_color_map(state);
                update_flag.remove(UpdateFlag::UPDATE_SLICE_COLOR_MAP);
            }

            assert!(update_flag.is_empty());

            let result = renderer.run_ui_and_paint(state, emulator, window, update_flag)?;

            if emulator.initialized() && state.auto_play {
                if cfg!(target_os = "windows") {
                    window.request_redraw();
                } else {
                    return Ok(EventResult::RepaintNow);
                }
            }

            Ok(result)
        } else {
            Ok(EventResult::Wait)
        }
    }

    fn on_resumed(&mut self, event_loop: &ActiveEventLoop) -> Result<EventResult> {
        if self.window.is_none() {
            self.initialize(event_loop)?;
        }
        Ok(EventResult::RepaintNow)
    }

    fn on_window_event(&mut self, event: winit::event::WindowEvent) -> Result<EventResult> {
        self.update(None);
        if let Some(window) = self.window.as_ref().cloned() {
            match event {
                winit::event::WindowEvent::RedrawRequested => self.run_ui_and_paint(&window),
                _ => {
                    if let Some(renderer) = &mut self.renderer {
                        Ok(renderer.on_window_event(&event, &window, &self.state))
                    } else {
                        Ok(EventResult::Wait)
                    }
                }
            }
        } else {
            Ok(EventResult::Wait)
        }
    }

    fn on_user_event(&mut self, event: UserEvent) -> Result<EventResult> {
        match event {
            UserEvent::RequestRepaint {
                cumulative_pass_nr,
                when,
            } => {
                self.update(None);
                if let Some(renderer) = &mut self.renderer {
                    return Ok(renderer.on_user_event(when, cumulative_pass_nr));
                }
            }
            UserEvent::Server(signal) => {
                self.update(Some(signal));
                if self.renderer.is_some() {
                    return Ok(EventResult::RepaintNow);
                }
            }
        }

        Ok(EventResult::Wait)
    }

    fn on_device_event(&mut self, event: winit::event::DeviceEvent) -> Result<EventResult> {
        self.update(None);
        if let Some(renderer) = &mut self.renderer {
            Ok(renderer.on_device_event(event))
        } else {
            Ok(EventResult::Wait)
        }
    }

    fn handle_event_result(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_result: Result<EventResult>,
    ) {
        let mut exit = false;

        let combined_result = event_result.and_then(|event_result| match event_result {
            EventResult::Wait => {
                event_loop.set_control_flow(ControlFlow::Wait);
                Ok(event_result)
            }
            EventResult::RepaintNow => {
                if cfg!(target_os = "windows") {
                    if let Some(ref window) = self.window.as_ref().cloned() {
                        self.update(None);
                        self.run_ui_and_paint(window)
                    } else {
                        Ok(event_result)
                    }
                } else {
                    self.windows_next_repaint_time = Some(Instant::now());
                    Ok(event_result)
                }
            }
            EventResult::RepaintNext => {
                self.windows_next_repaint_time = Some(Instant::now());
                Ok(event_result)
            }
            EventResult::RepaintAt(repaint_time) => {
                self.windows_next_repaint_time = Some(
                    self.windows_next_repaint_time
                        .map_or(repaint_time, |last| last.min(repaint_time)),
                );
                Ok(event_result)
            }
            EventResult::Exit => {
                exit = true;
                Ok(event_result)
            }
        });

        if let Err(err) = combined_result {
            exit = true;
            self.run_result = Err(err);
        };

        if exit {
            event_loop.exit();
        }

        self.check_redraw_requests(event_loop);
    }

    fn check_redraw_requests(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let Some(next_repaint_time) = self.windows_next_repaint_time {
            if now >= next_repaint_time {
                self.windows_next_repaint_time = None;
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(next_repaint_time));
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for Simulator {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let event_result = self.on_resumed(event_loop);
        self.handle_event_result(event_loop, event_result);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let event_result = self.on_window_event(event);
        self.handle_event_result(event_loop, event_result);
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::event::StartCause,
    ) {
        self.check_redraw_requests(event_loop);
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: UserEvent) {
        let event_result = self.on_user_event(event);
        self.handle_event_result(event_loop, event_result);
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let event_result = self.on_device_event(event);
        self.handle_event_result(event_loop, event_result);
    }

    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.handle_event_result(event_loop, Ok(EventResult::Wait));
    }

    fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {
        if let Some(server) = self.server.take() {
            let _ = server.shutdown();
        }
    }
}

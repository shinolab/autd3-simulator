mod transducers;

use std::{f32::consts::PI, sync::Arc};

use autd3_core::{
    gain::{Drive, Phase},
    link::{RxMessage, TxMessage},
};
use autd3_driver::{ethercat::DcSysTime, geometry::Geometry};
use autd3_firmware_emulator::CPUEmulator;
use parking_lot::RwLock;

use crate::ULTRASOUND_PERIOD_COUNT;

pub struct Emulator<'a> {
    pub cpu: &'a mut CPUEmulator,
    pub transducers: &'a mut [transducers::TransState],
    pub visible: &'a mut bool,
    pub enable: &'a mut bool,
    pub thermal: &'a mut bool,
    pub drive_buffer: &'a mut [Drive],
    pub phase_buffer: &'a mut [Phase],
    pub output_mask_buffer: &'a mut [bool],
}

pub struct EmulatorWrapper {
    cpus: Vec<CPUEmulator>,
    transducers: transducers::Transducers,
    rx_buf: Arc<RwLock<Vec<RxMessage>>>,
    visible: Vec<bool>,
    enable: Vec<bool>,
    thermal: Vec<bool>,
    drive_buffer: Vec<Vec<Drive>>,
    phase_buffer: Vec<Vec<Phase>>,
    output_mask_buffer: Vec<Vec<bool>>,
}

impl EmulatorWrapper {
    pub fn new(rx_buf: Arc<RwLock<Vec<RxMessage>>>) -> Self {
        Self {
            cpus: Default::default(),
            transducers: transducers::Transducers::new(),
            rx_buf,
            visible: Default::default(),
            enable: Default::default(),
            thermal: Default::default(),
            drive_buffer: Vec::new(),
            phase_buffer: Vec::new(),
            output_mask_buffer: Vec::new(),
        }
    }

    pub fn initialized(&self) -> bool {
        !self.cpus.is_empty()
    }

    pub fn transducers(&self) -> &transducers::Transducers {
        &self.transducers
    }

    pub fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = Emulator<'a>> {
        self.cpus
            .iter_mut()
            .zip(self.visible.iter_mut())
            .zip(self.enable.iter_mut())
            .zip(self.thermal.iter_mut())
            .zip(self.transducers.devices())
            .zip(self.drive_buffer.iter_mut())
            .zip(self.phase_buffer.iter_mut())
            .zip(self.output_mask_buffer.iter_mut())
            .map(
                |(
                    (
                        (((((cpu, visible), enable), thermal), transducers), drive_buffer),
                        phase_buffer,
                    ),
                    output_mask_buffer,
                )| Emulator {
                    cpu,
                    transducers,
                    visible,
                    enable,
                    thermal,
                    drive_buffer,
                    phase_buffer,
                    output_mask_buffer,
                },
            )
    }

    pub fn update(&mut self, system_time: DcSysTime) {
        self.cpus.iter_mut().for_each(|cpu| {
            cpu.update_with_sys_time(system_time);
        });
        if self
            .cpus
            .iter()
            .any(autd3_firmware_emulator::CPUEmulator::should_update)
        {
            self.rx_buf
                .write()
                .iter_mut()
                .zip(self.cpus.iter())
                .for_each(|(d, s)| {
                    *d = s.rx();
                });
        }
    }

    pub fn update_transducers(&mut self, mod_enable: bool) {
        self.iter_mut().for_each(|emulator| {
            let cpu = emulator.cpu;
            let stm_segment = cpu.fpga().current_stm_segment();
            let idx = if cpu.fpga().stm_cycle(stm_segment) == 1 {
                0
            } else {
                cpu.fpga().current_stm_idx()
            };
            cpu.fpga().drives_at_inplace(
                stm_segment,
                idx,
                emulator.phase_buffer,
                emulator.output_mask_buffer,
                emulator.drive_buffer,
            );
            let mod_segment = cpu.fpga().current_mod_segment();
            let m = if mod_enable {
                let mod_idx = cpu.fpga().current_mod_idx();
                cpu.fpga().modulation_at(mod_segment, mod_idx)
            } else {
                u8::MAX
            };
            emulator
                .transducers
                .iter_mut()
                .zip(emulator.drive_buffer)
                .for_each(|(tr, d)| {
                    tr.amp = (PI * cpu.fpga().to_pulse_width(d.intensity, m).pulse_width() as f32
                        / ULTRASOUND_PERIOD_COUNT as f32)
                        .sin();
                    tr.phase = d.phase.radian();
                });
        });
    }

    pub fn initialize(&mut self, geometry: &Geometry) {
        self.cpus = geometry
            .iter()
            .map(|dev| CPUEmulator::new(dev.idx(), dev.num_transducers()))
            .collect();
        self.transducers.initialize(geometry);
        *self.rx_buf.write() = self.cpus.iter().map(|cpu| cpu.rx()).collect();
        self.visible = vec![true; self.cpus.len()];
        self.enable = vec![true; self.cpus.len()];
        self.thermal = vec![false; self.cpus.len()];
        self.drive_buffer = self
            .cpus
            .iter()
            .map(|cpu| vec![Drive::NULL; cpu.num_transducers()])
            .collect();
        self.phase_buffer = self
            .cpus
            .iter()
            .map(|cpu| vec![Phase::ZERO; cpu.num_transducers()])
            .collect();
        self.output_mask_buffer = self
            .cpus
            .iter()
            .map(|cpu| vec![true; cpu.num_transducers()])
            .collect();
    }

    pub fn update_geometry(&mut self, geometry: &Geometry) {
        self.transducers.update_geometry(geometry);
    }

    pub fn send(&mut self, tx: &[TxMessage]) {
        self.cpus.iter_mut().for_each(|cpu| {
            cpu.send(tx);
        });
        self.rx_buf
            .write()
            .iter_mut()
            .zip(self.cpus.iter())
            .for_each(|(d, s)| {
                *d = s.rx();
            });
    }

    pub fn clear(&mut self) {
        self.cpus.clear();
        self.transducers.clear();
        self.visible.clear();
        self.enable.clear();
        self.thermal.clear();
        self.drive_buffer.clear();
        self.phase_buffer.clear();
        self.output_mask_buffer.clear();
    }
}

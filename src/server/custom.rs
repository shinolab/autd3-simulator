// # Protocol Specification
//
// ## Message Types
//
// - `0x01`: Configure Geometry
// - `0x02`: Update Geometry
// - `0x03`: Send Data
// - `0x04`: Read Data
// - `0x05`: Close
// - `0x10`: Hello (handshake)
//
// ## Response Status Codes
//
// - `0x00`: OK
// - `0xFF`: Error
//
// ## Message Formats
//
// ### Hello (Handshake)
// Request:
// - 1 byte: message type (0x10)
// - 2 bytes: protocol version (u16, little-endian)
// - 11 bytes: magic string `AUTD3REMOTE`
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// ### Configure/Update Geometry
// Request:
// - 1 byte: message type (0x01 or 0x02)
// - 4 bytes: number of devices (u32, little-endian)
// - For each device:
//   - 12 bytes: position (3x f32, little-endian)
//   - 16 bytes: rotation quaternion (w, i, j, k as f32, little-endian)
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// ### Send Data
// Request:
// - 1 byte: message type (0x03)
// - Raw TxMessage data for each device
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// ### Read Data
// Request:
// - 1 byte: message type (0x04)
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
// - Raw RxMessage data for each device
//
// ### Close
// Request:
// - 1 byte: message type (0x05)
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// ### Error Response
// - 1 byte: status (0xFF = Error)
// - 4 bytes: error message length (u32, little-endian)
// - N bytes: error message (UTF-8 string)

pub(crate) const MSG_CONFIG_GEOMETRY: u8 = 0x01;
pub(crate) const MSG_UPDATE_GEOMETRY: u8 = 0x02;
pub(crate) const MSG_SEND_DATA: u8 = 0x03;
pub(crate) const MSG_READ_DATA: u8 = 0x04;
pub(crate) const MSG_CLOSE: u8 = 0x05;
pub(crate) const MSG_HELLO: u8 = 0x10;

pub(crate) const MSG_OK: u8 = 0x00;
pub(crate) const MSG_ERROR: u8 = 0xFF;

pub(crate) const REMOTE_PROTOCOL_VERSION: u16 = 1;
pub(crate) const REMOTE_PROTOCOL_MAGIC: &[u8; 11] = b"AUTD3REMOTE";

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};

use autd3_core::link::{RxMessage, TxMessage};
use autd3_driver::geometry::Geometry;
use winit::event_loop::EventLoopProxy;

use crate::error::{Result, SimulatorError};
use crate::event::{Signal, UserEvent};

pub struct CustomServer {
    rx_buf: Arc<RwLock<Vec<RxMessage>>>,
    rx_data: Option<Vec<u8>>,
    tx_buffer_queue: Receiver<Vec<TxMessage>>,
    proxy: EventLoopProxy<UserEvent>,
    num_devices: usize,
}

unsafe impl Send for CustomServer {}
unsafe impl Sync for CustomServer {}

impl CustomServer {
    pub fn new(
        rx_buf: Arc<RwLock<Vec<RxMessage>>>,
        tx_buffer_queue: Receiver<Vec<TxMessage>>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            rx_buf,
            rx_data: None,
            tx_buffer_queue,
            proxy,
            num_devices: 0,
        }
    }

    pub fn run(mut self, listener: TcpListener) -> Result<()> {
        loop {
            let (stream, _addr) = listener.accept()?;
            let _ = self.handle_client(stream);
        }
    }

    fn handle_client(&mut self, mut stream: TcpStream) -> Result<()> {
        let mut handshake_completed = false;

        loop {
            let mut msg_type = [0u8; size_of::<u8>()];
            if stream.read_exact(&mut msg_type).is_err() {
                break;
            }

            let msg = msg_type[0];
            let result = if msg == MSG_HELLO {
                if handshake_completed {
                    Err(SimulatorError::server_error("Handshake already completed"))
                } else {
                    match Self::handle_handshake(&mut stream) {
                        Ok(()) => {
                            handshake_completed = true;
                            Ok(())
                        }
                        Err(e) => {
                            eprintln!("Handshake failed: {}", e);
                            Err(e)
                        }
                    }
                }
            } else if !handshake_completed {
                Err(SimulatorError::server_error(
                    "Handshake is required before sending commands",
                ))
            } else {
                match msg {
                    MSG_CONFIG_GEOMETRY => self.handle_config_geometry(&mut stream),
                    MSG_UPDATE_GEOMETRY => self.handle_update_geometry(&mut stream),
                    MSG_SEND_DATA => self.handle_send_data(&mut stream),
                    MSG_READ_DATA => self.handle_read_data(&mut stream),
                    MSG_CLOSE => self.handle_close(&mut stream),
                    other => Err(SimulatorError::server_error(format!(
                        "Unknown message type: {}",
                        other
                    ))),
                }
            };

            match result {
                Ok(()) => {
                    if msg == MSG_CLOSE {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error handling client request: {}", e);
                    let _ = Self::send_error(&mut stream, e);
                    if !handshake_completed || msg == MSG_CLOSE {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_handshake(stream: &mut TcpStream) -> Result<()> {
        let mut version_buf = [0u8; size_of::<u16>()];
        stream.read_exact(&mut version_buf)?;
        let version = u16::from_le_bytes(version_buf);
        if version != REMOTE_PROTOCOL_VERSION {
            return Err(SimulatorError::server_error(format!(
                "Unsupported protocol version: {}",
                version
            )));
        }

        let mut magic_buf = [0u8; REMOTE_PROTOCOL_MAGIC.len()];
        stream.read_exact(&mut magic_buf)?;
        if &magic_buf != REMOTE_PROTOCOL_MAGIC {
            eprintln!("Invalid client magic: {:?}", magic_buf);
            return Err(SimulatorError::server_error("Invalid client magic"));
        }

        stream.write_all(&[MSG_OK])?;
        Ok(())
    }

    fn handle_config_geometry(&mut self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream)?;
        self.num_devices = geometry.num_devices();
        self.proxy
            .send_event(UserEvent::Server(Signal::ConfigGeometry(geometry)))
            .map_err(|_e| SimulatorError::server_error("Simulator is closed"))?;
        stream.write_all(&[MSG_OK])?;
        Ok(())
    }

    fn handle_update_geometry(&self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream)?;
        self.proxy
            .send_event(UserEvent::Server(Signal::UpdateGeometry(geometry)))
            .map_err(|_e| SimulatorError::server_error("Simulator is closed"))?;
        stream.write_all(&[MSG_OK])?;
        Ok(())
    }

    fn read_geometry(&self, stream: &mut TcpStream) -> Result<Geometry> {
        let mut num_devices_buf = [0u8; 4];
        stream.read_exact(&mut num_devices_buf)?;
        let num_devices = u32::from_le_bytes(num_devices_buf);
        Ok(autd3_core::geometry::Geometry::new(
            (0..num_devices)
                .map(|_| {
                    let mut pos_buf = [0u8; 12];
                    stream.read_exact(&mut pos_buf)?;
                    let x = f32::from_le_bytes([pos_buf[0], pos_buf[1], pos_buf[2], pos_buf[3]]);
                    let y = f32::from_le_bytes([pos_buf[4], pos_buf[5], pos_buf[6], pos_buf[7]]);
                    let z = f32::from_le_bytes([pos_buf[8], pos_buf[9], pos_buf[10], pos_buf[11]]);

                    let mut rot_buf = [0u8; 16];
                    stream.read_exact(&mut rot_buf)?;
                    let w = f32::from_le_bytes([rot_buf[0], rot_buf[1], rot_buf[2], rot_buf[3]]);
                    let i = f32::from_le_bytes([rot_buf[4], rot_buf[5], rot_buf[6], rot_buf[7]]);
                    let j = f32::from_le_bytes([rot_buf[8], rot_buf[9], rot_buf[10], rot_buf[11]]);
                    let k =
                        f32::from_le_bytes([rot_buf[12], rot_buf[13], rot_buf[14], rot_buf[15]]);

                    Ok(autd3_core::devices::AUTD3 {
                        pos: autd3_core::geometry::Point3::new(x, y, z),
                        rot: autd3_core::geometry::UnitQuaternion { w, i, j, k },
                    }
                    .into())
                })
                .collect::<Result<Vec<_>>>()?,
        ))
    }

    fn handle_send_data(&self, stream: &mut TcpStream) -> Result<()> {
        let mut tx_data = match self.tx_buffer_queue.try_recv() {
            Ok(data) => data,
            Err(_) => {
                vec![TxMessage::new(); self.num_devices]
            }
        };
        unsafe {
            let buf = std::slice::from_raw_parts_mut(
                tx_data.as_mut_ptr() as *mut u8,
                tx_data.len() * std::mem::size_of::<TxMessage>(),
            );
            stream.read_exact(buf)?;
        }

        self.proxy
            .send_event(UserEvent::Server(Signal::Send(tx_data)))
            .map_err(|_e| SimulatorError::server_error("Simulator is closed"))?;

        stream.write_all(&[MSG_OK])?;
        Ok(())
    }

    fn handle_read_data(&mut self, stream: &mut TcpStream) -> Result<()> {
        let rx_data = {
            let mut rx_data = match self.rx_data.take() {
                Some(buf) if buf.len() == self.num_devices * std::mem::size_of::<RxMessage>() => {
                    buf
                }
                _ => vec![0x00; self.num_devices * std::mem::size_of::<RxMessage>()],
            };
            let rx = self.rx_buf.read().unwrap();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    rx.as_ptr(),
                    rx_data.as_mut_ptr() as *mut RxMessage,
                    rx.len(),
                );
            }
            rx_data
        };

        stream.write_all(&[MSG_OK])?;
        stream.write_all(&rx_data)?;

        self.rx_data = Some(rx_data);

        Ok(())
    }

    fn handle_close(&self, stream: &mut TcpStream) -> Result<()> {
        self.proxy
            .send_event(UserEvent::Server(Signal::Close))
            .map_err(|_e| SimulatorError::server_error("Simulator is closed"))?;
        stream.write_all(&[MSG_OK])?;
        Ok(())
    }

    fn send_error(stream: &mut TcpStream, error: SimulatorError) -> std::io::Result<()> {
        let error_msg = error.to_string();
        let error_bytes = error_msg.as_bytes();
        let error_len = error_bytes.len() as u32;

        let mut buffer = Vec::with_capacity(size_of::<u8>() + size_of::<u32>() + error_bytes.len());
        buffer.push(MSG_ERROR);
        buffer.extend_from_slice(&error_len.to_le_bytes());
        buffer.extend_from_slice(error_bytes);

        stream.write_all(&buffer)
    }
}

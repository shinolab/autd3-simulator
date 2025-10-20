use std::{error::Error, fmt};

#[derive(Debug)]
pub enum SimulatorError {
    OsError(winit::error::OsError),
    ExternalError(winit::error::ExternalError),
    EventLoopError(winit::error::EventLoopError),
    CreateSurfaceError(wgpu::CreateSurfaceError),
    RequestDeviceError(wgpu::RequestDeviceError),
    RequestAdapterError(wgpu::RequestAdapterError),
    SurfaceError(wgpu::SurfaceError),
    ImageError(image::ImageError),
    IoError(std::io::Error),
    NoSuitableFormat,
    ServerError(String),
}

impl fmt::Display for SimulatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OsError(e) => write!(f, "{}", e),
            Self::ExternalError(e) => write!(f, "{}", e),
            Self::EventLoopError(e) => write!(f, "{}", e),
            Self::CreateSurfaceError(e) => write!(f, "{}", e),
            Self::RequestDeviceError(e) => write!(f, "{}", e),
            Self::RequestAdapterError(e) => write!(f, "{}", e),
            Self::SurfaceError(e) => write!(f, "{}", e),
            Self::ImageError(e) => write!(f, "{}", e),
            Self::IoError(e) => write!(f, "{}", e),
            Self::NoSuitableFormat => write!(f, "Failed to select proper surface texture format"),
            Self::ServerError(e) => write!(f, "{}", e),
        }
    }
}

impl Error for SimulatorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OsError(e) => Some(e),
            Self::ExternalError(e) => Some(e),
            Self::EventLoopError(e) => Some(e),
            Self::CreateSurfaceError(e) => Some(e),
            Self::RequestDeviceError(e) => Some(e),
            Self::RequestAdapterError(e) => Some(e),
            Self::SurfaceError(e) => Some(e),
            Self::ImageError(e) => Some(e),
            Self::IoError(e) => Some(e),
            Self::NoSuitableFormat => None,
            Self::ServerError(_) => None,
        }
    }
}

impl From<winit::error::OsError> for SimulatorError {
    fn from(e: winit::error::OsError) -> Self {
        Self::OsError(e)
    }
}

impl From<winit::error::ExternalError> for SimulatorError {
    fn from(e: winit::error::ExternalError) -> Self {
        Self::ExternalError(e)
    }
}

impl From<winit::error::EventLoopError> for SimulatorError {
    fn from(e: winit::error::EventLoopError) -> Self {
        Self::EventLoopError(e)
    }
}

impl From<wgpu::CreateSurfaceError> for SimulatorError {
    fn from(e: wgpu::CreateSurfaceError) -> Self {
        Self::CreateSurfaceError(e)
    }
}

impl From<wgpu::RequestDeviceError> for SimulatorError {
    fn from(e: wgpu::RequestDeviceError) -> Self {
        Self::RequestDeviceError(e)
    }
}

impl From<wgpu::RequestAdapterError> for SimulatorError {
    fn from(e: wgpu::RequestAdapterError) -> Self {
        Self::RequestAdapterError(e)
    }
}

impl From<wgpu::SurfaceError> for SimulatorError {
    fn from(e: wgpu::SurfaceError) -> Self {
        Self::SurfaceError(e)
    }
}

impl From<image::ImageError> for SimulatorError {
    fn from(e: image::ImageError) -> Self {
        Self::ImageError(e)
    }
}

impl From<std::io::Error> for SimulatorError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

pub type Result<T> = std::result::Result<T, SimulatorError>;

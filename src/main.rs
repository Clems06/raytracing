mod app;
mod raytracer;
mod shapes;
mod load_files;

use crate::app::AppState;

fn main() -> Result<(), impl std::error::Error>{
    let event_loop = winit::event_loop::EventLoop::new().unwrap();

    let mut app = AppState::new();
    event_loop.run_app(&mut app)
}

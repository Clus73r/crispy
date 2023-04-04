use std::vec;
use super::sphere::Sphere;
use super::camera::Camera;

pub struct Scene {
    pub camera: Camera,
    pub spheres: Vec<Sphere>,
}
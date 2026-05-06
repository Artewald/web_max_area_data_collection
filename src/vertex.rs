use std::{collections::VecDeque, f32::consts::PI, fs::read_to_string, sync::{Arc, Mutex}};

use bytemuck::{Pod, Zeroable};
use nalgebra_glm as glm;
#[cfg(not(target_arch = "wasm32"))]
use rand::{seq::IndexedRandom, Rng};
#[cfg(not(target_arch = "wasm32"))]
use rand_distr::{Beta, Distribution};
use serde::{Deserialize, Serialize};

use crate::metrics::get_triangulation_statistics;

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable, Serialize, Deserialize)]
pub struct Vertex {
    pub position: glm::Vec2,
    pub _padding: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub enum TriangulationType {
    Fan,
    Strip,
    MaxArea,
}

impl TriangulationType {
    pub fn next(&mut self) {
        match self {
            Self::Fan => *self = Self::Strip,
            Self::Strip => *self = Self::MaxArea,
            Self::MaxArea => *self = Self::Fan,
        }
    }
}

pub fn generate_circle(triangulation_type: &TriangulationType, num_points: usize, radius: f32) -> (Vec<Vertex>, Vec<u32>) {
    let triangulation = match triangulation_type {
        TriangulationType::Fan => generate_circle_type_one(radius, num_points),
        TriangulationType::Strip => generate_circle_type_two(radius, num_points),
        TriangulationType::MaxArea => generate_circle_type_three(radius, num_points),
    };
    triangulation
}

#[cfg(not(target_arch = "wasm32"))]
fn generate_random_triangle(radius: f32, num_points: usize) -> (Vec<Vertex>, Vec<u32>) {
    let vertices = calculate_circle_points(radius, num_points).into_iter().map(|v| Vertex { position: v, _padding: 0.0 }).collect::<Vec<_>>();
    let mut indices = Vec::new();

    let mut rng = rand::rng();
    let mut queue = VecDeque::new();

    let s1 = rng.random_range(0..vertices.len()-2);
    let s2 = {
        let mut s = rng.random_range(s1..vertices.len()-1);
        while s == s1 {
            s = rng.random_range(s1..vertices.len()-1);
        }
        s
    };
    let s3 = {
        let mut s = rng.random_range(s2..vertices.len());
        while s == s1 || s == s2 {
            s = rng.random_range(s2..vertices.len());
        }
        s
    };

    indices.append(&mut vec![s1 as u32, s2 as u32, s3 as u32]);

    push_valid_edge(s1, s2, &mut queue, num_points);
    push_valid_edge(s2, s3, &mut queue, num_points);
    push_valid_edge(s3, s1, &mut queue, num_points);

    let edge_betas = [1.0, 0.5, 1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8];
    let edge_beta = *edge_betas.choose(&mut rng).unwrap();
    while let Some((start, end)) = queue.pop_front() {
        let len = end.checked_sub(start).unwrap_or((end + num_points) - start);

        let beta = Beta::new(edge_beta, edge_beta).unwrap();
        let t = beta.sample(&mut rng);
        let i = (start + 1 + ((len-2) as f64 * t).round() as usize) % num_points;

        indices.append(&mut vec![start as u32, i as u32, end as u32]);
        push_valid_edge(start, i, &mut queue, num_points);
        push_valid_edge(i, end, &mut queue, num_points);
    }

    (vertices, indices)
}

fn push_valid_edge(a: usize, b: usize, q: &mut VecDeque<(usize, usize)>, num_points: usize) {
    let (start, end) = if a < b { (a, b) } else { (a, b+num_points) };
    if start.abs_diff(end) > 1 {
        q.push_back((start % num_points, end % num_points));
    }
}

pub fn generate_circle_type_one(radius: f32, num_points: usize) -> (Vec<Vertex>, Vec<u32>) {
    let points = calculate_circle_points(radius, num_points - 1);
    let mut vertices = vec![Vertex { position: glm::Vec2::new(0.0, 0.0), _padding: 0.0}];
    let mut indices = Vec::new();

    for point in points {
        vertices.push(Vertex { position: point, _padding: 0.0});
    }

    for i in 0..(num_points - 2) {
        indices.push(0);
        indices.push((i + 1) as u32);
        indices.push((i + 2) as u32);
    }

    indices.push(0);
    indices.push((num_points - 1) as u32);
    indices.push(1);

    indices.reverse();
    (vertices, indices)
}

pub fn generate_circle_type_two(radius: f32, num_points: usize) -> (Vec<Vertex>, Vec<u32>) {
    let points = calculate_circle_points(radius, num_points);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for point in points {
        vertices.push(Vertex { position: point, _padding: 0.0});
    }

    indices.push(0);
    indices.push(num_points as u32 - 1);
    indices.push(1);

    let half = num_points / 2;
    for i in 1..(half + (num_points % 2) - 1) as u32 {
        indices.push(i);
        indices.push(num_points as u32 - i);
        indices.push(num_points as u32 - i - 1);
        indices.push(i);
        indices.push(num_points as u32 - i - 1);
        indices.push(i + 1);
    }

    indices.push(num_points as u32 / 2);
    indices.push(num_points as u32 / 2 - 1);
    indices.push(num_points as u32 / 2 + 1);

    (vertices, indices)
}

pub fn generate_circle_type_three(radius: f32, num_points: usize) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(num_points);
    let mut indices = Vec::with_capacity(num_points * 3);
    let mut edge_queue = VecDeque::with_capacity(num_points);

    let angles: Vec<f32> = match num_points % 3 {
        0 => vec![0.0, 120.0, 240.0],
        1 => vec![0.0, 90.0, 180.0, 270.0],
        _ => vec![0.0, 60.0, 120.0, 180.0, 240.0, 300.0],
    };

    let positions: Vec<_> = angles.iter()
        .map(|&angle| {
            let rad = angle.to_radians();
            glm::Vec2::new(rad.sin() * radius, rad.cos() * radius)
        })
        .collect();
    vertices.push(Vertex { position: positions[0], _padding: 0.0 });
    for (i, &position) in positions.iter().enumerate().skip(1) {
        vertices.push(Vertex { position, _padding: 0.0 });
        if i+1 < positions.len() {
            indices.extend_from_slice(&[0, i as u32, (i + 1) as u32]);
        }
        edge_queue.push_back((i - 1, i));
    }
    edge_queue.push_back((positions.len() - 1, 0));

    while !edge_queue.is_empty() && vertices.len() < num_points {
        let (p1, p2) = edge_queue.pop_front().unwrap();
        let mut mid = mid_point(vertices[p1].position, vertices[p2].position);
        extend_to_circle(&mut mid, radius);
        let mid_index = vertices.len();
        vertices.push(Vertex { position: mid, _padding: 0.0 });
        indices.extend_from_slice(&[p1 as u32, mid_index as u32, p2 as u32]);
        edge_queue.push_back((p1, mid_index));
        edge_queue.push_back((mid_index, p2));
    }

    (vertices, indices)
}

fn mid_point(p1: glm::Vec2, p2: glm::Vec2) -> glm::Vec2 {
    glm::Vec2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0)
}

fn extend_to_circle(p1: &mut glm::Vec2, radius: f32) {
    let length = (p1.x * p1.x + p1.y * p1.y).sqrt();
    *p1 = glm::Vec2::new(p1.x / length * radius, p1.y / length * radius);
}

fn calculate_circle_points(radius: f32, num_points: usize) -> Vec<glm::Vec2> {
    (0..num_points).map(|i| {
        let angle = i as f32 * 2.0 * PI / num_points as f32;
        glm::Vec2::new(radius * angle.cos(), radius * angle.sin())
    }).collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn generate_random_triangles_in_buckets(min_edge_length: f32, max_edge_length: f32, num_buckets: usize, num_items_in_buckets: usize, circle_radius: f32, num_points_in_circle: usize) -> Vec<(Vec<Vertex>, Vec<u32>)> {
    let step_size = (max_edge_length-min_edge_length) / num_buckets as f32;
    let bucket_size_and_remaining_count = Arc::new(Mutex::new({
        let mut data = Vec::with_capacity(num_buckets);
        for n in 0..num_buckets {
            let bottom_value = min_edge_length + step_size*n as f32;
            let top_value = min_edge_length + step_size*(n+1) as f32;
            let value_range = bottom_value..top_value;
            data.push((value_range, num_items_in_buckets));
        }
        data
    }));

    let triangulations = Arc::new(Mutex::new(Vec::with_capacity(num_buckets*num_items_in_buckets)));

    let num_cpus = num_cpus::get().checked_sub(1).unwrap_or(1);
    let mut handles = Vec::with_capacity(num_cpus);

    for i in 0..num_cpus {
        let i_c = i;
        let bucket_size_and_remaining_count_c = bucket_size_and_remaining_count.clone();
        let triangulations_c = triangulations.clone();
        let handle = std::thread::spawn(move || {
            let i = i_c;
            while !bucket_size_and_remaining_count_c.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
                let (vertices, indices) = generate_random_triangle(circle_radius, num_points_in_circle);
                let statistics = get_triangulation_statistics(&vertices, &indices);

                {
                    let mut lock = bucket_size_and_remaining_count_c.lock().unwrap_or_else(|e| e.into_inner());
                    match &mut lock.iter_mut().filter(|(range, _)| range.contains(&statistics.edge_lengths.sum)).next() {
                        Some((_, num_items)) => {
                            let mut triangulation_lock = triangulations_c.lock().unwrap_or_else(|e| e.into_inner());
                            triangulation_lock.push((vertices, indices));
                            *num_items -= 1;
                        },
                        None => (),
                    };
                    lock.retain(|(_, num_items)| *num_items > 0);
                    if i == 0 {
                        println!("Remaining buckets = {}", lock.len());
                        dbg!(&lock);
                    }
                }
            }
        });
        handles.push(handle);
    }

    handles.into_iter().for_each(|h| { let _ = h.join(); });

    match Arc::into_inner(triangulations) {
        Some(x) => x.into_inner().unwrap_or_else(|e| e.into_inner()),
        None => panic!("The triangulations value is still most likely shared across multiple threads. This means that an assumption in the code is broken!"),
    }
}

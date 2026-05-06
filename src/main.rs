use std::fs::write;

#[cfg(not(target_arch = "wasm32"))]
use web_max_area_data_collection::vertex::generate_random_triangles_in_buckets;
use web_max_area_data_collection::{NumVerticesCalculator, run, vertex::{Vertex, generate_circle_type_one, generate_circle_type_three, generate_circle_type_two}};

fn main() {
    // run().unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    create_triangulation_file();
    // #[cfg(not(target_arch = "wasm32"))]
    // read_triangulation_from_file();
}

// #[cfg(not(target_arch = "wasm32"))]
// fn read_triangulation_from_file() {
//     let bin_data = include_bytes!("triangulation_data.bin");
//     let data: Vec<(Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
//     dbg!(data.len());
//     let bin_data = include_bytes!("triangulation_data.bin");
//     let data: Vec<(Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
//     dbg!(data.len());
// }

#[cfg(not(target_arch = "wasm32"))]
fn create_triangulation_file() {
    let vertex_counter = NumVerticesCalculator::Power(2);
    let start_exponent = 8;
    let end_exponent = 21;

    let mut data = Vec::new();
    for exponent in start_exponent..=end_exponent {

        let num_vertices = vertex_counter.get_num_vertices(exponent);
        let (v1, i1) = generate_circle_type_one(0.75, num_vertices);
        let (v2, i2) = generate_circle_type_two(0.75, num_vertices);
        let (v3, i3) = generate_circle_type_three(0.75, num_vertices);
        data.push((v1, i1));
        data.push((v2, i2));
        data.push((v3, i3));
    }

    let data = postcard::to_allocvec(&data).unwrap();
    write("./src/fan_stripe_max_area.bin", data).unwrap();

    let data = generate_random_triangles_in_buckets(200.0, 500_000.0, 10, 10, 0.75, 262_144);
    let data = postcard::to_allocvec(&data).unwrap();
    write("./src/random_triangulations_262_144.bin", data).unwrap();
}

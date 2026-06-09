use log::info;
#[cfg(not(target_arch = "wasm32"))]
use rand::seq::SliceRandom;
use std::fs::write;
#[cfg(not(target_arch = "wasm32"))]
use web_max_area_data_collection::vertex::{
    TriangulationType, generate_random_triangles_in_buckets,
};
use web_max_area_data_collection::{
    no_surface::collect_data, run, vertex::{
        Vertex, generate_circle_type_one, generate_circle_type_three, generate_circle_type_two,
    }
};

fn main() {
    // #[cfg(not(target_arch = "wasm32"))]
    // create_triangulation_file();
    // #[cfg(not(target_arch = "wasm32"))]
    // read_triangulation_from_file();
    // run().unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        unsafe { std::env::set_var("RUST_LOG", "info"); }
        env_logger::init();
        pollster::block_on(collect_data()).unwrap();
    }
}

// #[cfg(not(target_arch = "wasm32"))]
// fn read_triangulation_from_file() {
//     let bin_data = include_bytes!("fan_stripe_max_area.bin");
//     let data: Vec<(Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
//     dbg!(data.len());
//     let bin_data = include_bytes!("random_triangulations_262_144.bin");
//     let data: Vec<(Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
//     dbg!(data.len());
// }

#[cfg(not(target_arch = "wasm32"))]
fn create_triangulation_file() {
    let vertex_power = 2_usize;
    let start_exponent = 8;
    let end_exponent = 21;

    let mut data = Vec::new();
    let mut rng = rand::rng();
    for exponent in start_exponent..=end_exponent {
        let num_vertices = vertex_power.pow(exponent);
        let (v1, i1) = generate_circle_type_one(0.75, num_vertices);
        let mut i1 = i1
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect::<Vec<_>>();
        i1.shuffle(&mut rng);
        let i1 = i1.into_iter().flatten().collect::<Vec<_>>();

        let (v2, i2) = generate_circle_type_two(0.75, num_vertices);
        let mut i2 = i2
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect::<Vec<_>>();
        i2.shuffle(&mut rng);
        let i2 = i2.into_iter().flatten().collect::<Vec<_>>();

        let (v3, i3) = generate_circle_type_three(0.75, num_vertices);
        let mut i3 = i3
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect::<Vec<_>>();
        i3.shuffle(&mut rng);
        let i3 = i3.into_iter().flatten().collect::<Vec<_>>();
        assert!(v1.len() == v2.len() && v1.len() == v3.len());

        data.push((TriangulationType::Fan, v1, i1));
        data.push((TriangulationType::Strip, v2, i2));
        data.push((TriangulationType::MaxArea, v3, i3));
    }

    let data = postcard::to_allocvec(&data).unwrap();
    write("./src/fan_stripe_max_area.bin", data).unwrap();

    let data = generate_random_triangles_in_buckets(100.0, 100_000.0, 10, 10, 0.75, 65_536)
        .into_iter()
        .map(|(v, i)| (TriangulationType::Random, v, i))
        .collect::<Vec<_>>();
    let data = postcard::to_allocvec(&data).unwrap();
    write("./src/random_triangulations_65_536.bin", data).unwrap();
}

use std::fmt::Display;

use crate::vertex::Vertex;
use nalgebra_glm as glm;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStats {
    pub sum: f32,
    pub avg: f32,
    pub min: f32,
    pub max: f32,
    pub top_5_percent_sum: f32,
    pub bottom_95_percent_sum: f32,
}

impl MetricStats {
    pub fn new(mut v: Vec<f32>) -> Self {
        let sum = v.iter().sum::<f32>();
        let count = v.len() as f32;
        let avg = sum/count;
        let min = v.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = v.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let top_5_percent_index_length = count * 0.05;
        v.sort_by(|a, b| a.total_cmp(b));
        let top_5_percent_sum = v[(count - top_5_percent_index_length - 1.0).max(0.0) as usize .. count as usize].iter().sum::<f32>();
        let bottom_95_percent_sum = v[0..(count - top_5_percent_index_length - 1.0).max(0.0) as usize].iter().sum::<f32>();
        Self {
            sum,
            avg,
            min,
            max,
            top_5_percent_sum,
            bottom_95_percent_sum,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TriangulationStatistics {
    pub raw_data: Vec<((f32, f32), u32)>,
    pub aspect_ratio: MetricStats,
    pub skewness: MetricStats,
    pub interpolation_quality: MetricStats,
    pub mean_ratio: MetricStats,
    pub shape_regularities: MetricStats,
    pub edge_lengths: MetricStats,
    pub area: MetricStats,
}

impl Display for TriangulationStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Area barchart (relative):\n")?;
        let max_amount = self.raw_data.iter().map(|(_, a)| a).max().unwrap();
        let max_bars = 25;
        let amount_per_bar = *max_amount as f32 / max_bars as f32;
        for ((start, stop), amount) in &self.raw_data {
            let amount_bars = (*amount as f32 / amount_per_bar).floor() as u32;
            f.write_str(&format!("\t{:.2e}..{:.2e}: {} ({})\n", start, stop, "\u{2588}".repeat(amount_bars as usize), *amount))?;
        }
        // f.write_str(&format!("Area ratio: {}\n", self.area_ratio))?;
        // f.write_str(&format!("Coefficient of variation: {}\n", self.coefficient_of_variation))?;
        // f.write_str(&format!("Gini coefficient: {}\n", self.gini_coefficient))?;
        // f.write_str(&format!("Top 5% area (%): {:.2}\n", self.top_5_percent_area * 100.0))?;
        // f.write_str(&format!("Bottom 95% area (%): {:.2}", self.bottom_95_percent_area * 100.0))?;
        f.write_str("TODO: implement display of TriangulationStatistics");

        Ok(())
    }
}

pub fn get_triangulation_statistics(vertices: &[Vertex], indices: &[u32]) -> TriangulationStatistics {
    let mut aspect_ratios = Vec::with_capacity(indices.len() / 3);
    let mut skewnesses = Vec::with_capacity(indices.len() / 3);
    let mut interp_quals = Vec::with_capacity(indices.len() / 3);
    let mut mean_ratios = Vec::with_capacity(indices.len() / 3);
    let mut shape_regs = Vec::with_capacity(indices.len() / 3);
    let mut edge_len_vec = Vec::with_capacity(indices.len() / 3);

    let mut areas = Vec::with_capacity(indices.len() / 3);

    for idxs in indices.chunks_exact(3) {
        let v1 = vertices[idxs[0] as usize];
        let v2 = vertices[idxs[1] as usize];
        let v3 = vertices[idxs[2] as usize];

        let e12 = v2.position - v1.position;
        let e13 = v3.position - v1.position;
        let e23 = v3.position - v2.position;

        let a = glm::length(&e23);
        let b = glm::length(&e13);
        let c = glm::length(&e12);

        edge_len_vec.push(a+b+c);

        let area = (e12.x * e13.y - e13.x * e12.y).abs() / 2.0;
        areas.push(area);

        let s = (a + b + c) / 2.0;
        let r = area / s;
        let big_r = (a * b * c) / (4.0 * area);
        let aspect_ratio = r / big_r;
        aspect_ratios.push(aspect_ratio);

        let min_angle = e12.angle(&e13).min(e12.angle(&e23)).min(e13.angle(&e23));
        let max_angle = e12.angle(&e13).max(e12.angle(&e23)).max(e13.angle(&e23));
        let skewness = min_angle.sin() / (max_angle.sin()+1.0e-7);
        skewnesses.push(skewness);

        let interpolation_quality = area / big_r.powi(2);
        interp_quals.push(interpolation_quality);

        let j_det = e12.x * e13.y - e12.y * e13.x;
        let trace_mt_m = e12.dot(&e12) + e13.dot(&e13);
        let mean_ratio = j_det / trace_mt_m;
        mean_ratios.push(mean_ratio);

        let shape_regularity = (3.0 * area) / (a.powi(2) + b.powi(2) + c.powi(2));
        shape_regs.push(shape_regularity);
    }

    let min_area = areas.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let max_area = areas.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let num_buckets = 10;
    let bucket_threshold = (max_area - min_area) / num_buckets as f32;
    let mut area_barchart = Vec::with_capacity(num_buckets);
    (0..num_buckets).into_iter().for_each(|i| {
        let i = i as f32;
        area_barchart.push(((min_area+(i*bucket_threshold), min_area+((i+1.0)*bucket_threshold)), 0_u32));
    });
    areas.iter().for_each(|a| {
        let bucket_nr = (((a-min_area) / bucket_threshold).floor() as usize).min(num_buckets-1);
        area_barchart[bucket_nr].1 += 1;
    });

    TriangulationStatistics {
        raw_data: area_barchart,
        aspect_ratio: MetricStats::new(aspect_ratios),
        skewness: MetricStats::new(skewnesses),
        interpolation_quality: MetricStats::new(interp_quals),
        mean_ratio: MetricStats::new(mean_ratios),
        shape_regularities: MetricStats::new(shape_regs),
        edge_lengths: MetricStats::new(edge_len_vec),
        area: MetricStats::new(areas),
    }
}

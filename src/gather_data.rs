use std::fs::{read_to_string, write};

use chrono::{DateTime, Local};
use log::{info, warn};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use rfd::FileHandle;
use serde::{ser::SerializeStruct, Serialize, Serializer};

use crate::{metrics::TriangulationStatistics, State, vertex::{TriangulationType, Vertex}};

pub const WARMUP_MS: i64 = 5 * 1000;
pub const DATA_GATHER_MS: i64 = 5 * 1000;


#[derive(Debug, Clone, Serialize)]
enum DataCollectionStage {
    Inactive,
    Warmup,
    Active,
    Finished,
}

#[derive(Debug, Clone)]
pub struct InformationGathered {
    pub triangulation_type: TriangulationType,
    pub num_vertices: usize,
    pub num_frames: usize,
    pub total_time_ms: u64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub metrics: TriangulationStatistics,
}

macro_rules! write_metric_fields {
    ($state:expr, $prefix:expr, $metrics:expr) => {
        $state.serialize_field(concat!($prefix, "_sum"), &$metrics.sum)?;
        $state.serialize_field(concat!($prefix, "_avg"), &$metrics.avg)?;
        $state.serialize_field(concat!($prefix, "_min"), &$metrics.min)?;
        $state.serialize_field(concat!($prefix, "_max"), &$metrics.max)?;
        $state.serialize_field(concat!($prefix, "_top_5_percent_sum"), &$metrics.top_5_percent_sum)?;
        $state.serialize_field(concat!($prefix, "_bottom_95_percent_sum"), &$metrics.bottom_95_percent_sum)?;
    };
}

impl Serialize for InformationGathered {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer {
        const NUM_COLUMNS: usize = 6 + 7 * 6; // 6 metrics from this struct + num metrics from TriangulationStatistics * num fields per metric
        let mut state = serializer.serialize_struct("InformationGathered", NUM_COLUMNS)?;

        let t = match &self.triangulation_type {
            TriangulationType::Fan => "TriangulationType::Fan",
            TriangulationType::Strip => "TriangulationType::Strip",
            TriangulationType::MaxArea => "TriangulationType::MaxArea",
            TriangulationType::Random => "TriangulationType::Random",
        };
        state.serialize_field("triangulation_type", t)?;
        state.serialize_field("num_vertices", &self.num_vertices)?;
        state.serialize_field("num_frames", &self.num_frames)?;
        state.serialize_field("total_time_ms", &self.total_time_ms)?;
        state.serialize_field("frame_width", &self.frame_width)?;
        state.serialize_field("frame_height", &self.frame_height)?;

        write_metric_fields!(state, "aspect_ratio", self.metrics.aspect_ratio);
        write_metric_fields!(state, "skewness", self.metrics.skewness);
        write_metric_fields!(state, "interpolation_quality", self.metrics.interpolation_quality);
        write_metric_fields!(state, "mean_ratio", self.metrics.mean_ratio);
        write_metric_fields!(state, "shape_regularities", self.metrics.shape_regularities);
        write_metric_fields!(state, "edge_lengths", self.metrics.edge_lengths);
        write_metric_fields!(state, "area", self.metrics.area);

        state.end()
    }
}

impl InformationGathered {
    pub fn new(state: &State) -> Self {
        let stats = state.get_current_triangulation_statistics();
        let frame_size = state.get_window_size();
        Self {
            triangulation_type: state.get_triangulation_type(),
            num_vertices: state.get_num_vertices(),
            num_frames: 0,
            total_time_ms: 0,
            frame_width: frame_size.0,
            frame_height: frame_size.1,
            metrics: stats,
        }
    }
}

pub struct GatherData {
    timer: Option<DateTime<Local>>,
    gathered_data: Vec::<InformationGathered>,
    current_information_gathered: InformationGathered,
    stage: DataCollectionStage,
    current_triangulation: usize,
}

impl GatherData {
    pub fn new(state: &mut State) -> Self {
        state.reset_to_default_triangulation();

        Self {
            timer: None,
            gathered_data: Vec::new(),
            current_information_gathered: InformationGathered::new(state),
            stage: DataCollectionStage::Inactive,
            current_triangulation: 0,
        }
    }

    pub fn update(&mut self, state: &mut State) {
        match self.stage {
            DataCollectionStage::Inactive => {
                self.stage = DataCollectionStage::Warmup;
            },
            DataCollectionStage::Warmup => {
                if let Some(timer) = self.timer {
                    let elapsed = Local::now()-timer;
                    if elapsed.num_milliseconds().abs() >= WARMUP_MS {
                        self.stage = DataCollectionStage::Active;
                        self.timer = None;
                    }
                } else {
                    self.timer = Some(Local::now());
                }
            },
            DataCollectionStage::Active => {
                if let Some(timer) = self.timer {
                    self.current_information_gathered.num_frames += 1;
                    let elapsed_ms = (Local::now() - timer).num_milliseconds().abs();
                    if elapsed_ms >= DATA_GATHER_MS {
                        self.stage = DataCollectionStage::Inactive;
                        self.timer = None;

                        self.current_information_gathered.total_time_ms = u64::try_from(elapsed_ms).unwrap();

                        let finished = state.next_triangulation();

                        let mut data = InformationGathered::new(state);

                        std::mem::swap(&mut data, &mut self.current_information_gathered);
                        self.gathered_data.push(data);

                        if finished {
                            self.stage = DataCollectionStage::Finished;
                            let mut wtr = csv::Writer::from_writer(vec![]);
                            self.gathered_data.iter().for_each(|d| {
                                wtr.serialize(d).unwrap();
                            });
                            let csv_bytes = &wtr.into_inner().unwrap();
                            let csv_data = String::from_utf8_lossy(csv_bytes);
                            let size = state.get_render_size();
                            let gpu_name = state.get_gpu_name();

                            let mut data = String::new();
                            data += &format!("Name: {},\nResolution: {}x{},\n\n", gpu_name, size.0, size.1);
                            data += &csv_data;
                            let data = data.into_bytes();
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                rfd::FileDialog::new().save_file().and_then(|p| write(p, data).ok()).unwrap();
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                wasm_bindgen_futures::spawn_local(
                                    async move {
                                        use rfd::AsyncFileDialog;

                                        let file_handle = AsyncFileDialog::new().set_file_name("rendering_metrics.csv").save_file().await.unwrap_throw();
                                        file_handle.write(&data).await.unwrap_throw();
                                    }
                                )
                            }
                        }
                    }
                } else {
                    self.timer = Some(Local::now());
                }
            },
            DataCollectionStage::Finished => (),
        }
    }
}

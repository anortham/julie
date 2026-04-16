use super::{DataPlaneHealth, HealthLevel, ProjectionState, SystemStatus};

pub(super) fn overall_from_planes(
    control_level: HealthLevel,
    data_level: HealthLevel,
    runtime_level: HealthLevel,
) -> HealthLevel {
    if data_level == HealthLevel::Unavailable {
        HealthLevel::Unavailable
    } else if control_level == HealthLevel::Ready && runtime_level == HealthLevel::Ready {
        data_level
    } else {
        HealthLevel::Degraded
    }
}

pub(super) fn overall_from_levels(levels: &[HealthLevel]) -> HealthLevel {
    if levels.contains(&HealthLevel::Unavailable) {
        HealthLevel::Unavailable
    } else if levels.contains(&HealthLevel::Degraded) {
        HealthLevel::Degraded
    } else {
        HealthLevel::Ready
    }
}

pub(super) fn readiness_from_data_plane(data_plane: &DataPlaneHealth) -> SystemStatus {
    if data_plane.canonical_store.symbol_count <= 0
        && data_plane.canonical_store.level == HealthLevel::Unavailable
    {
        SystemStatus::NotReady
    } else if data_plane.search_projection.state == ProjectionState::Ready {
        SystemStatus::FullyReady {
            symbol_count: data_plane.canonical_store.symbol_count.max(1),
        }
    } else {
        SystemStatus::SqliteOnly {
            symbol_count: data_plane.canonical_store.symbol_count.max(1),
        }
    }
}

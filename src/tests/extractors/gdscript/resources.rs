use super::*;
use crate::extractors::base::{SymbolKind, Visibility};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_resource_handling_custom_resources_and_serialization_patterns() {
        let gd_code = r#"
extends Resource
class_name GameData

@export var version: String = "1.0"
@export var player_data: PlayerData
@export var world_data: WorldData
@export var settings: GameSettings

@export var levels: Array[LevelData] = []
@export var items: Array[ItemData] = []
@export var achievements: Array[AchievementData] = []

func serialize() -> Dictionary:
	return {
		"version": version,
		"player_data": player_data.serialize() if player_data else null,
		"world_data": world_data.serialize() if world_data else null,
		"settings": settings.serialize() if settings else null,
		"levels": levels.map(func(level): return level.serialize()),
		"items": items.map(func(item): return item.serialize()),
		"achievements": achievements.map(func(achievement): return achievement.serialize())
	}

func deserialize(data: Dictionary):
	version = data.get("version", "1.0")

	if data.has("player_data") and data["player_data"]:
		player_data = PlayerData.new()
		player_data.deserialize(data["player_data"])

	if data.has("world_data") and data["world_data"]:
		world_data = WorldData.new()
		world_data.deserialize(data["world_data"])

	if data.has("settings") and data["settings"]:
		settings = GameSettings.new()
		settings.deserialize(data["settings"])

	levels.clear()
	for level_data in data.get("levels", []):
		var level = LevelData.new()
		level.deserialize(level_data)
		levels.append(level)

class_name PlayerData
extends Resource

@export var name: String = ""
@export var level: int = 1
@export var experience: int = 0
@export var health: int = 100
@export var mana: int = 50
@export var position: Vector3 = Vector3.ZERO
@export var inventory: InventoryData
@export var stats: CharacterStats
@export var unlocked_abilities: Array[String] = []

func serialize() -> Dictionary:
	var data = {
		"name": name,
		"level": level,
		"experience": experience,
		"health": health,
		"mana": mana,
		"position": {"x": position.x, "y": position.y, "z": position.z},
		"unlocked_abilities": unlocked_abilities.duplicate()
	}

	if inventory:
		data["inventory"] = inventory.serialize()

	if stats:
		data["stats"] = stats.serialize()

	return data

func deserialize(data: Dictionary):
	name = data.get("name", "")
	level = max(1, data.get("level", 1))
	experience = max(0, data.get("experience", 0))
	health = clamp(data.get("health", 100), 0, 999)
	mana = clamp(data.get("mana", 50), 0, 999)

	var pos_data = data.get("position", {})
	position = Vector3(
		pos_data.get("x", 0.0),
		pos_data.get("y", 0.0),
		pos_data.get("z", 0.0)
	)

	unlocked_abilities = data.get("unlocked_abilities", []).duplicate()

	if data.has("inventory"):
		inventory = InventoryData.new()
		inventory.deserialize(data["inventory"])

	if data.has("stats"):
		stats = CharacterStats.new()
		stats.deserialize(data["stats"])

extends Node

var loaded_resources: Dictionary = {}
var resource_cache: Dictionary = {}
var loading_queue: Array = []
var max_cache_size: int = 100

signal resource_loaded(path: String, resource: Resource)
signal resource_failed(path: String, error: String)

func load_resource_async(path: String, type_hint: String = "") -> Resource:
	if resource_cache.has(path):
		return resource_cache[path]

	if path in loading_queue:
		while path in loading_queue:
			await get_tree().process_frame

	loading_queue.append(path)

	var loader := ResourceLoader.load_threaded_request(path, type_hint)
	if loader == null:
		resource_failed.emit(path, "Failed to start loading")
		loading_queue.erase(path)
		return null

	while true:
		var status = ResourceLoader.load_threaded_get_status(path)
		match status:
			ResourceLoader.THREAD_LOAD_LOADED:
				var resource = ResourceLoader.load_threaded_get(path)
				_cache_resource(path, resource)
				resource_loaded.emit(path, resource)
				loading_queue.erase(path)
				return resource
			ResourceLoader.THREAD_LOAD_FAILED:
				resource_failed.emit(path, "Thread load failed")
				loading_queue.erase(path)
				return null

		await get_tree().process_frame

func _cache_resource(path: String, resource: Resource):
	if resource_cache.size() >= max_cache_size:
		_evict_newest_resource()

	resource_cache[path] = resource
	loaded_resources[path] = {
		"resource": resource,
		"timestamp": Time.get_ticks_msec()
	}

func _evict_newest_resource():
	var newest_path = ""
	var newest_time = -inf

	for path in resource_cache.keys():
		var timestamp = loaded_resources[path]["timestamp"]
		if timestamp > newest_time:
			newest_time = timestamp
			newest_path = path

	resource_cache.erase(newest_path)
	loaded_resources.erase(newest_path)

class_name GameConfig
extends Resource

@export_category("Graphics")
@export var resolution: Vector2i = Vector2i(1920, 1080)
@export var fullscreen: bool = true
@export_range(0.5, 2.0) var render_scale: float = 1.0
@export_enum("Low", "Medium", "High", "Ultra") var quality_preset: int = 2

@export_category("Audio")
@export_range(0.0, 1.0) var master_volume: float = 0.8
@export_range(0.0, 1.0) var music_volume: float = 0.6
@export_range(0.0, 1.0) var sfx_volume: float = 0.7
@export var mute: bool = false

@export_category("Input")
@export var mouse_sensitivity: float = 1.0
@export var invert_y_axis: bool = false
@export var key_bindings: Dictionary = {
	"move_left": KEY_A,
	"move_right": KEY_D,
	"jump": KEY_SPACE,
	"attack": KEY_F
}

func apply_settings():
	_apply_graphics_settings()
	_apply_audio_settings()
	_apply_input_settings()

func _apply_graphics_settings():
	OS.window_size = resolution
	OS.window_fullscreen = fullscreen

func _apply_audio_settings():
	AudioServer.set_bus_volume_db(AudioServer.get_bus_index("Master"), linear_to_db(master_volume))
	AudioServer.set_bus_mute(AudioServer.get_bus_index("Master"), mute)

func _apply_input_settings():
	Input.set_mouse_sensitivity(mouse_sensitivity)
	Input.set_mouse_mode(invert_y_axis ? Input.MOUSE_MODE_CAPTURED : Input.MOUSE_MODE_VISIBLE)

func save_to_file(path: String):
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_var(serialize())
	file.close()

func load_from_file(path: String) -> bool:
	if not FileAccess.file_exists(path):
		return false

	var file := FileAccess.open(path, FileAccess.READ)
	var data = file.get_var()
	file.close()
	deserialize(data)
	return true

func serialize() -> Dictionary:
	return {
		"resolution": resolution,
		"fullscreen": fullscreen,
		"render_scale": render_scale,
		"quality_preset": quality_preset,
		"master_volume": master_volume,
		"music_volume": music_volume,
		"sfx_volume": sfx_volume,
		"mute": mute,
		"mouse_sensitivity": mouse_sensitivity,
		"invert_y_axis": invert_y_axis,
		"key_bindings": key_bindings.duplicate()
	}

func deserialize(data: Dictionary):
	resolution = data.get("resolution", Vector2i(1920, 1080))
	fullscreen = data.get("fullscreen", true)
	render_scale = clamp(data.get("render_scale", 1.0), 0.5, 2.0)
	quality_preset = clamp(data.get("quality_preset", 2), 0, 3)
	master_volume = clamp(data.get("master_volume", 0.8), 0.0, 1.0)
	music_volume = clamp(data.get("music_volume", 0.6), 0.0, 1.0)
	sfx_volume = clamp(data.get("sfx_volume", 0.7), 0.0, 1.0)
	mute = data.get("mute", false)
	mouse_sensitivity = data.get("mouse_sensitivity", 1.0)
	invert_y_axis = data.get("invert_y_axis", false)
	key_bindings = data.get("key_bindings", key_bindings).duplicate()
"#;

        let symbols = extract_symbols(gd_code);

        let game_data = symbols.iter().find(|s| s.name == "GameData");
        assert!(game_data.is_some());
        let game_data = game_data.unwrap();
        assert_eq!(game_data.kind, SymbolKind::Class);

        let version = symbols
            .iter()
            .find(|s| s.name == "version" && s.parent_id == Some(game_data.id.clone()));
        assert!(version.is_some());

        let serialize = symbols
            .iter()
            .find(|s| s.name == "serialize" && s.parent_id == Some(game_data.id.clone()));
        assert!(serialize.is_some());
        assert!(serialize
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("-> Dictionary"));

        let deserialize = symbols
            .iter()
            .find(|s| s.name == "deserialize" && s.parent_id == Some(game_data.id.clone()));
        assert!(deserialize.is_some());

        let player_data = symbols.iter().find(|s| s.name == "PlayerData");
        assert!(player_data.is_some());
        assert_eq!(player_data.unwrap().kind, SymbolKind::Class);

        let name = symbols
            .iter()
            .find(|s| s.name == "name" && s.parent_id == Some(player_data.unwrap().id.clone()));
        assert!(name.is_some());

        let unlocked_abilities = symbols.iter().find(|s| s.name == "unlocked_abilities");
        assert!(unlocked_abilities.is_some());

        let loaded_resources = symbols.iter().find(|s| s.name == "loaded_resources");
        assert!(loaded_resources.is_some());

        let resource_loaded = symbols.iter().find(|s| s.name == "resource_loaded");
        assert!(resource_loaded.is_some());
        assert_eq!(resource_loaded.unwrap().kind, SymbolKind::Event);

        let load_resource_async = symbols.iter().find(|s| s.name == "load_resource_async");
        assert!(load_resource_async.is_some());

        let cache_resource = symbols.iter().find(|s| s.name == "_cache_resource");
        assert!(cache_resource.is_some());
        assert_eq!(
            cache_resource.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private
        );

        let evict_resource = symbols.iter().find(|s| s.name == "_evict_newest_resource");
        assert!(evict_resource.is_some());

        let game_config = symbols.iter().find(|s| s.name == "GameConfig");
        assert!(game_config.is_some());
        assert_eq!(game_config.unwrap().kind, SymbolKind::Class);

        let resolution = symbols.iter().find(|s| {
            s.name == "resolution" && s.parent_id == Some(game_config.unwrap().id.clone())
        });
        assert!(resolution.is_some());

        let render_scale = symbols.iter().find(|s| s.name == "render_scale");
        assert!(render_scale.is_some());

        let master_volume = symbols.iter().find(|s| s.name == "master_volume");
        assert!(master_volume.is_some());

        let mouse_sensitivity = symbols.iter().find(|s| s.name == "mouse_sensitivity");
        assert!(mouse_sensitivity.is_some());

        let key_bindings = symbols.iter().find(|s| s.name == "key_bindings");
        assert!(key_bindings.is_some());

        let apply_settings = symbols.iter().find(|s| s.name == "apply_settings");
        assert!(apply_settings.is_some());

        let apply_graphics_settings = symbols
            .iter()
            .find(|s| s.name == "_apply_graphics_settings");
        assert!(apply_graphics_settings.is_some());
        assert_eq!(
            apply_graphics_settings
                .unwrap()
                .visibility
                .as_ref()
                .unwrap(),
            &Visibility::Private
        );

        let save_to_file = symbols.iter().find(|s| s.name == "save_to_file");
        assert!(save_to_file.is_some());

        let load_from_file = symbols.iter().find(|s| s.name == "load_from_file");
        assert!(load_from_file.is_some());
    }
}

use super::*;
use crate::extractors::base::{SymbolKind, Visibility};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_game_development_patterns_node_references_and_godot_specific_constructs() {
        let gd_code = r#"
extends Control

# Node references and paths
@onready var player: CharacterBody2D = $Player
@onready var ui_manager: UIManager = $"UI/UIManager"
@onready var camera: Camera2D = $Player/Camera2D
@onready var world_environment: WorldEnvironment = $WorldEnvironment

# Resource preloading
const PlayerScene: PackedScene = preload("res://scenes/Player.tscn")
const EnemyScene: PackedScene = preload("res://scenes/Enemy.tscn")
const BulletScene: PackedScene = preload("res://scenes/Bullet.tscn")

# Resource loading
var enemy_texture: Texture2D = load("res://textures/enemy.png")
var background_music: AudioStream = load("res://audio/background.ogg")

# Game state management
enum GameState {
	MENU,
	PLAYING,
	PAUSED,
	GAME_OVER
}

var current_state: GameState = GameState.MENU
var score: int = 0
var level: int = 1
var lives: int = 3

# Input handling
func _input(event: InputEvent):
	match event:
		InputEventKey():
			_handle_keyboard_input(event)
		InputEventMouseButton():
			_handle_mouse_input(event)
		InputEventJoypadButton():
			_handle_controller_input(event)

func _handle_keyboard_input(event: InputEventKey):
	if event.pressed:
		match event.keycode:
			KEY_SPACE:
				_shoot()
			KEY_P:
				_toggle_pause()
			KEY_ESCAPE:
				_open_menu()

func _unhandled_key_input(event: InputEventKey):
	if event.pressed and event.keycode == KEY_F11:
		_toggle_fullscreen()

# Scene management
func change_scene(scene_path: String):
	get_tree().change_scene_to_file(scene_path)

func load_scene_async(scene_path: String):
	ResourceLoader.load_threaded_request(scene_path)
	var progress: Array = []

	while true:
		var status = ResourceLoader.load_threaded_get_status(scene_path, progress)
		match status:
			ResourceLoader.THREAD_LOAD_LOADED:
				var scene = ResourceLoader.load_threaded_get(scene_path)
				get_tree().change_scene_to_packed(scene)
				break
			ResourceLoader.THREAD_LOAD_FAILED:
				print("Failed to load scene: ", scene_path)
				break

		await get_tree().process_frame

# Node manipulation
func spawn_enemy(position: Vector2):
	var enemy: Node2D = EnemyScene.instantiate()
	enemy.global_position = position
	get_parent().add_child(enemy)

	# Connect enemy signals
	enemy.connect("enemy_died", _on_enemy_died)
	enemy.connect("player_hit", _on_player_hit)

func find_nodes_by_group(group_name: String) -> Array[Node]:
	return get_tree().get_nodes_in_group(group_name)

func cleanup_expired_objects():
	var bullets = get_tree().get_nodes_in_group("bullets")
	for bullet in bullets:
		if bullet.has_method("is_expired") and bullet.is_expired():
			bullet.queue_free()

# Physics and collision
func _on_area_2d_body_entered(body: Node2D):
	if body.is_in_group("player"):
		_collect_powerup()
	elif body.is_in_group("enemy"):
		_damage_enemy(body)

func _on_rigid_body_2d_body_entered(body: Node, from: RigidBody2D):
	var collision_force: float = from.linear_velocity.length()
	if collision_force > 100.0:
		_create_explosion(from.global_position)

# Animation and tweening
func animate_ui_element(element: Control, target_position: Vector2):
	var tween: Tween = create_tween()
	tween.parallel().tween_property(element, "position", target_position, 0.5)
	tween.parallel().tween_property(element, "modulate:a", 1.0, 0.3)
	await tween.finished

func shake_camera(intensity: float, duration: float):
	var camera: Camera2D = get_viewport().get_camera_2d()
	var original_position: Vector2 = camera.global_position

	var shake_timer: float = 0.0
	while shake_timer < duration:
		var offset: Vector2 = Vector2(
			randf_range(-intensity, intensity),
			randf_range(-intensity, intensity)
		)
		camera.global_position = original_position + offset
		shake_timer += get_process_delta_time()
		await get_tree().process_frame

	camera.global_position = original_position

# Audio management
@onready var audio_manager: AudioStreamPlayer = $AudioManager
@onready var sfx_player: AudioStreamPlayer2D = $SFXPlayer

func play_sound(sound_stream: AudioStream, volume: float = 0.0):
	sfx_player.stream = sound_stream
	sfx_player.volume_db = volume
	sfx_player.play()

func play_music(music_stream: AudioStream, fade_in: bool = false):
	if fade_in:
		audio_manager.volume_db = -80.0
		audio_manager.stream = music_stream
		audio_manager.play()

		var tween: Tween = create_tween()
		tween.tween_property(audio_manager, "volume_db", 0.0, 2.0)
	else:
		audio_manager.stream = music_stream
		audio_manager.play()

# Save/Load system
const SAVE_FILE: String = "user://savegame.save"

func save_game():
	var save_dict: Dictionary = {
		"player_name": player_name,
		"level": level,
		"score": score,
		"position": player.global_position,
		"inventory": inventory.get_items(),
		"timestamp": Time.get_unix_time_from_system()
	}

	var save_file: FileAccess = FileAccess.open(SAVE_FILE, FileAccess.WRITE)
	if save_file:
		save_file.store_string(JSON.stringify(save_dict))
		save_file.close()
		return true
	return false

func load_game() -> bool:
	if not FileAccess.file_exists(SAVE_FILE):
		return false

	var save_file: FileAccess = FileAccess.open(SAVE_FILE, FileAccess.READ)
	if save_file:
		var json_string: String = save_file.get_as_text()
		save_file.close()

		var json: JSON = JSON.new()
		var parse_result: Error = json.parse(json_string)

		if parse_result == OK:
			var save_dict: Dictionary = json.data
			player_name = save_dict.get("player_name", "Unknown")
			level = save_dict.get("level", 1)
			score = save_dict.get("score", 0)
			player.global_position = save_dict.get("position", Vector2.ZERO)
			return true

	return false

# Particle and visual effects
func create_explosion(position: Vector2, scale: float = 1.0):
	var explosion: GPUParticles2D = preload("res://effects/Explosion.tscn").instantiate()
	explosion.global_position = position
	explosion.process_material.scale_min = scale
	explosion.process_material.scale_max = scale * 1.5
	get_parent().add_child(explosion)

	explosion.emitting = true
	await explosion.finished
	explosion.queue_free()

# Custom drawing
func _draw():
	if debug_mode:
		_draw_debug_info()

func _draw_debug_info():
	var viewport_size: Vector2 = get_viewport_rect().size
	draw_rect(Rect2(Vector2.ZERO, viewport_size), Color.RED, false, 2.0)

	# Draw collision shapes
	for body in get_tree().get_nodes_in_group("enemies"):
		if body is RigidBody2D:
			var shape: CollisionShape2D = body.get_node("CollisionShape2D")
			if shape and shape.shape is RectangleShape2D:
				var rect_shape: RectangleShape2D = shape.shape
				var rect: Rect2 = Rect2(
					body.global_position - rect_shape.size / 2,
					rect_shape.size
				)
				draw_rect(rect, Color.YELLOW, false, 1.0)

# Signals and connections
signal game_state_changed(new_state: GameState)
signal score_updated(new_score: int)
signal level_completed(level_number: int, completion_time: float)

func _connect_signals():
	game_state_changed.connect(_on_game_state_changed)
	score_updated.connect(_on_score_updated)

	# Connect to scene tree signals
	get_tree().node_added.connect(_on_node_added)
	get_tree().node_removed.connect(_on_node_removed)

func _on_game_state_changed(new_state: GameState):
	match new_state:
		GameState.MENU:
			_show_main_menu()
		GameState.PLAYING:
			_start_gameplay()
		GameState.PAUSED:
			_pause_game()
		GameState.GAME_OVER:
			_show_game_over()

# Threading and async operations
func process_large_dataset(data: Array):
	var thread: Thread = Thread.new()
	thread.start(_background_processing.bind(data))

	while thread.is_alive():
		await get_tree().process_frame

	var result = thread.wait_to_finish()
	return result

func _background_processing(data: Array):
	var processed: Array = []
	for item in data:
		# Simulate heavy processing
		var result = _complex_calculation(item)
		processed.append(result)
	return processed
"#;

        let symbols = extract_symbols(gd_code);

        let player = symbols.iter().find(|s| s.name == "player");
        assert!(player.is_some());
        let player = player.unwrap();
        assert_eq!(
            player
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("CharacterBody2D")
        );
        assert!(player
            .signature
            .as_ref()
            .unwrap()
            .contains("@onready var player: CharacterBody2D = $Player"));

        let ui_manager = symbols.iter().find(|s| s.name == "ui_manager");
        assert!(ui_manager.is_some());
        assert_eq!(
            ui_manager
                .unwrap()
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("UIManager")
        );

        let camera = symbols.iter().find(|s| s.name == "camera");
        assert!(camera.is_some());
        assert!(camera
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("$Player/Camera2D"));

        let player_scene = symbols.iter().find(|s| s.name == "PlayerScene");
        assert!(player_scene.is_some());
        let player_scene = player_scene.unwrap();
        assert_eq!(player_scene.kind, SymbolKind::Constant);
        assert!(player_scene
            .signature
            .as_ref()
            .unwrap()
            .contains("preload(\"res://scenes/Player.tscn\")"));

        let enemy_texture = symbols.iter().find(|s| s.name == "enemy_texture");
        assert!(enemy_texture.is_some());

        let game_state = symbols.iter().find(|s| s.name == "GameState");
        assert!(game_state.is_some());
        assert_eq!(game_state.unwrap().kind, SymbolKind::Enum);

        let current_state = symbols.iter().find(|s| s.name == "current_state");
        assert!(current_state.is_some());
        assert_eq!(
            current_state
                .unwrap()
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("GameState")
        );

        let handle_keyboard_input = symbols.iter().find(|s| s.name == "_handle_keyboard_input");
        assert!(handle_keyboard_input.is_some());

        let change_scene = symbols.iter().find(|s| s.name == "change_scene");
        assert!(change_scene.is_some());

        let spawn_enemy = symbols.iter().find(|s| s.name == "spawn_enemy");
        assert!(spawn_enemy.is_some());

        let on_area_body_entered = symbols
            .iter()
            .find(|s| s.name == "_on_area_2d_body_entered");
        assert!(on_area_body_entered.is_some());

        let animate_ui_element = symbols.iter().find(|s| s.name == "animate_ui_element");
        assert!(animate_ui_element.is_some());

        let shake_camera = symbols.iter().find(|s| s.name == "shake_camera");
        assert!(shake_camera.is_some());

        let audio_manager = symbols.iter().find(|s| s.name == "audio_manager");
        assert!(audio_manager.is_some());
        assert_eq!(
            audio_manager
                .unwrap()
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("AudioStreamPlayer")
        );

        let play_sound = symbols.iter().find(|s| s.name == "play_sound");
        assert!(play_sound.is_some());

        let play_music = symbols.iter().find(|s| s.name == "play_music");
        assert!(play_music.is_some());

        let save_file = symbols.iter().find(|s| s.name == "SAVE_FILE");
        assert!(save_file.is_some());
        assert_eq!(save_file.unwrap().kind, SymbolKind::Constant);

        let save_game = symbols.iter().find(|s| s.name == "save_game");
        assert!(save_game.is_some());

        let load_game = symbols.iter().find(|s| s.name == "load_game");
        assert!(load_game.is_some());
        assert!(load_game
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("-> bool"));

        let create_explosion = symbols.iter().find(|s| s.name == "create_explosion");
        assert!(create_explosion.is_some());

        let draw = symbols.iter().find(|s| s.name == "_draw");
        assert!(draw.is_some());

        let draw_debug_info = symbols.iter().find(|s| s.name == "_draw_debug_info");
        assert!(draw_debug_info.is_some());

        let game_state_changed = symbols.iter().find(|s| s.name == "game_state_changed");
        assert!(game_state_changed.is_some());
        assert_eq!(game_state_changed.unwrap().kind, SymbolKind::Event);

        let score_updated = symbols.iter().find(|s| s.name == "score_updated");
        assert!(score_updated.is_some());

        let connect_signals = symbols.iter().find(|s| s.name == "_connect_signals");
        assert!(connect_signals.is_some());

        let on_game_state_changed = symbols.iter().find(|s| s.name == "_on_game_state_changed");
        assert!(on_game_state_changed.is_some());

        let process_large_dataset = symbols.iter().find(|s| s.name == "process_large_dataset");
        assert!(process_large_dataset.is_some());

        let background_processing = symbols.iter().find(|s| s.name == "_background_processing");
        assert!(background_processing.is_some());
        assert_eq!(
            background_processing.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private
        );
    }
}

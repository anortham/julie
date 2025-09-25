use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, SymbolOptions, Visibility};
use crate::extractors::gdscript::GDScriptExtractor;
use crate::tests::test_utils::{init_parser};

// Test helper function
fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser("gdscript", code);
    let mut extractor = GDScriptExtractor::new("gdscript".to_string(), "test.gd".to_string(), code.to_string());
    extractor.extract_symbols(&tree)
}

fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let tree = init_parser("gdscript", code);
    let mut extractor = GDScriptExtractor::new("gdscript".to_string(), "test.gd".to_string(), code.to_string());
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[cfg(test)]
mod gdscript_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_class_definitions_inheritance_and_built_in_node_types() {
        let gd_code = r#"
# Basic class definition
class_name Player
extends CharacterBody2D

# Class with inheritance from custom class
class_name Enemy
extends Actor

# Inner class definition
class HealthComponent:
	var max_health: int = 100
	var current_health: int

	func _init(health: int = 100):
		max_health = health
		current_health = health

	func take_damage(amount: int) -> bool:
		current_health -= amount
		return current_health <= 0

	func heal(amount: int):
		current_health = min(current_health + amount, max_health)

# Class with tool annotation
@tool
class_name CustomResource
extends Resource

# Class variables and properties
var health: int = 100
var mana: float = 50.0
var player_name: String = "Unknown"
var position: Vector2
var velocity: Vector3 = Vector3.ZERO

# Export variables (GDScript 4.0+)
@export var speed: float = 200.0
@export var jump_force: float = 400.0
@export var texture: Texture2D
@export_range(0, 100) var armor: int = 10
@export_flags("Fire", "Water", "Earth", "Air") var elements: int

# Legacy export syntax (GDScript 3.x)
export var legacy_speed: float = 150.0
export(int, 0, 100) var legacy_armor: int = 5
export(PackedScene) var bullet_scene: PackedScene

# OnReady variables
@onready var sprite: Sprite2D = $Sprite2D
@onready var collision: CollisionShape2D = $CollisionShape2D
@onready var animation_player: AnimationPlayer = get_node("AnimationPlayer")

# Constants and enums
const MAX_LIVES: int = 3
const GRAVITY: float = 980.0

enum State {
	IDLE,
	WALKING,
	JUMPING,
	FALLING,
	ATTACKING
}

enum Direction { LEFT = -1, RIGHT = 1 }

# Class with multiple inheritance indicators
class_name NetworkPlayer
extends Player

# Static variables
static var instance_count: int = 0
static var global_settings: Dictionary = {}

# Setget properties (GDScript 3.x style)
var _score: int = 0 setget set_score, get_score

func set_score(value: int):
	_score = max(0, value)
	_update_ui()

func get_score() -> int:
	return _score

# Modern property syntax (GDScript 4.0+)
var level: int = 1:
	set(value):
		level = clamp(value, 1, 100)
		level_changed.emit(level)
	get:
		return level

var experience: float:
	set(value):
		experience = max(0.0, value)
		if experience >= experience_to_next_level:
			level_up()
	get:
		return experience
"#;

        let symbols = extract_symbols(gd_code);

        // Class definitions
        let player = symbols.iter().find(|s| s.name == "Player");
        assert!(player.is_some());
        let player = player.unwrap();
        assert_eq!(player.kind, SymbolKind::Class);
        assert!(player.signature.as_ref().unwrap().contains("class_name Player"));
        assert_eq!(player.metadata.as_ref().and_then(|m| m.get("baseClass").map(|v| v.as_str().unwrap())), Some("CharacterBody2D"));

        let enemy = symbols.iter().find(|s| s.name == "Enemy");
        assert!(enemy.is_some());
        assert_eq!(enemy.unwrap().metadata.as_ref().and_then(|m| m.get("baseClass").map(|v| v.as_str().unwrap())), Some("Actor"));

        // Inner class
        let health_component = symbols.iter().find(|s| s.name == "HealthComponent");
        assert!(health_component.is_some());
        assert_eq!(health_component.unwrap().kind, SymbolKind::Class);

        // Inner class methods
        let take_damage = symbols.iter().find(|s| s.name == "take_damage" && s.parent_id == health_component.unwrap().parent_id);
        assert!(take_damage.is_some());
        assert_eq!(take_damage.unwrap().kind, SymbolKind::Method);
        assert!(take_damage.unwrap().signature.as_ref().unwrap().contains("func take_damage(amount: int) -> bool"));

        // Tool class
        let custom_resource = symbols.iter().find(|s| s.name == "CustomResource");
        assert!(custom_resource.is_some());
        assert!(custom_resource.unwrap().signature.as_ref().unwrap().contains("@tool"));

        // Class variables
        let health = symbols.iter().find(|s| s.name == "health");
        assert!(health.is_some());
        let health = health.unwrap();
        assert_eq!(health.kind, SymbolKind::Field);
        assert_eq!(health.metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("int"));
        assert!(health.signature.as_ref().unwrap().contains("var health: int = 100"));

        let player_name = symbols.iter().find(|s| s.name == "player_name");
        assert!(player_name.is_some());
        assert_eq!(player_name.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("String"));

        // Export variables
        let speed = symbols.iter().find(|s| s.name == "speed");
        assert!(speed.is_some());
        let speed = speed.unwrap();
        assert!(speed.signature.as_ref().unwrap().contains("@export var speed: float = 200.0"));
        assert_eq!(speed.visibility.as_ref().unwrap(), &Visibility::Public);

        let armor = symbols.iter().find(|s| s.name == "armor");
        assert!(armor.is_some());
        assert!(armor.unwrap().signature.as_ref().unwrap().contains("@export_range(0, 100)"));

        // Legacy export
        let legacy_speed = symbols.iter().find(|s| s.name == "legacy_speed");
        assert!(legacy_speed.is_some());
        assert!(legacy_speed.unwrap().signature.as_ref().unwrap().contains("export var legacy_speed"));

        // OnReady variables
        let sprite = symbols.iter().find(|s| s.name == "sprite");
        assert!(sprite.is_some());
        let sprite = sprite.unwrap();
        assert!(sprite.signature.as_ref().unwrap().contains("@onready var sprite: Sprite2D"));
        assert_eq!(sprite.metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Sprite2D"));

        // Constants
        let max_lives = symbols.iter().find(|s| s.name == "MAX_LIVES");
        assert!(max_lives.is_some());
        let max_lives = max_lives.unwrap();
        assert_eq!(max_lives.kind, SymbolKind::Constant);
        assert!(max_lives.signature.as_ref().unwrap().contains("const MAX_LIVES: int = 3"));

        // Enums
        let state_enum = symbols.iter().find(|s| s.name == "State");
        assert!(state_enum.is_some());
        assert_eq!(state_enum.unwrap().kind, SymbolKind::Enum);

        let direction_enum = symbols.iter().find(|s| s.name == "Direction");
        assert!(direction_enum.is_some());

        // Enum values
        let idle = symbols.iter().find(|s| s.name == "IDLE" && s.parent_id == state_enum.unwrap().parent_id);
        assert!(idle.is_some());
        assert_eq!(idle.unwrap().kind, SymbolKind::EnumMember);

        // Static variables
        let instance_count = symbols.iter().find(|s| s.name == "instance_count");
        assert!(instance_count.is_some());
        assert!(instance_count.unwrap().signature.as_ref().unwrap().contains("static var instance_count"));

        // Setget properties
        let score = symbols.iter().find(|s| s.name == "_score");
        assert!(score.is_some());
        assert!(score.unwrap().signature.as_ref().unwrap().contains("setget set_score, get_score"));

        let set_score = symbols.iter().find(|s| s.name == "set_score");
        assert!(set_score.is_some());
        assert_eq!(set_score.unwrap().kind, SymbolKind::Method);

        // Modern property syntax
        let level = symbols.iter().find(|s| s.name == "level");
        assert!(level.is_some());
        assert!(level.unwrap().signature.as_ref().unwrap().contains("var level: int = 1:"));
    }

    #[test]
    fn test_extract_function_definitions_built_in_callbacks_and_signal_declarations() {
        let gd_code = r#"
extends Node2D

# Signal declarations
signal health_changed(new_health: int)
signal player_died
signal item_collected(item_name: String, quantity: int)
signal level_completed(score: int, time: float)

# Built-in lifecycle functions
func _init():
	print("Object initialized")

func _ready():
	print("Node is ready")
	_setup_connections()

func _enter_tree():
	print("Entered scene tree")

func _exit_tree():
	print("Exited scene tree")

func _process(delta: float):
	_update_movement(delta)
	_check_boundaries()

func _physics_process(delta: float):
	_apply_physics(delta)

func _input(event: InputEvent):
	if event is InputEventKey:
		_handle_key_input(event)

func _unhandled_input(event: InputEvent):
	if event is InputEventMouseButton:
		_handle_mouse_click(event)

# Custom functions with various signatures
func simple_function():
	print("Simple function called")

func function_with_params(name: String, age: int, active: bool = true):
	print("Name: %s, Age: %d, Active: %s" % [name, age, active])

func function_with_return(x: float, y: float) -> Vector2:
	return Vector2(x, y)

func function_with_complex_return(data: Array) -> Dictionary:
	var result: Dictionary = {}
	for item in data:
		if item is String:
			result[item] = item.length()
	return result

# Static functions
static func calculate_distance(a: Vector2, b: Vector2) -> float:
	return a.distance_to(b)

static func create_random_color() -> Color:
	return Color(randf(), randf(), randf())

# Virtual functions
func _can_drop_data(position: Vector2, data) -> bool:
	return data is Dictionary and data.has("item_type")

func _drop_data(position: Vector2, data):
	if data.has("item_type"):
		_spawn_item(data.item_type, position)

# Private/internal functions (convention)
func _setup_connections():
	health_changed.connect(_on_health_changed)
	connect("player_died", _on_player_died)

func _update_movement(delta: float):
	var input_vector: Vector2 = Vector2.ZERO

	if Input.is_action_pressed("move_left"):
		input_vector.x -= 1
	if Input.is_action_pressed("move_right"):
		input_vector.x += 1
	if Input.is_action_pressed("move_up"):
		input_vector.y -= 1
	if Input.is_action_pressed("move_down"):
		input_vector.y += 1

	position += input_vector.normalized() * speed * delta

func _apply_physics(delta: float):
	velocity.y += gravity * delta
	velocity = move_and_slide(velocity)

# Signal handlers (conventional naming)
func _on_health_changed(new_health: int):
	if new_health <= 0:
		player_died.emit()

func _on_player_died():
	print("Game Over!")
	get_tree().change_scene_to_file("res://scenes/GameOver.tscn")

func _on_area_2d_body_entered(body: Node2D):
	if body.is_in_group("player"):
		item_collected.emit("coin", 1)

func _on_timer_timeout():
	_spawn_enemy()

# Coroutine functions
func fade_out(duration: float = 1.0):
	var tween: Tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, duration)
	await tween.finished

func move_to_position(target: Vector2, duration: float = 2.0):
	var tween: Tween = create_tween()
	tween.tween_property(self, "global_position", target, duration)
	await tween.finished

func async_load_scene(path: String):
	var loader: ResourceLoader = ResourceLoader.load_threaded_request(path)
	while ResourceLoader.load_threaded_get_status(path) != ResourceLoader.THREAD_LOAD_LOADED:
		await get_tree().process_frame
	return ResourceLoader.load_threaded_get(path)

# Function with yield (GDScript 3.x style)
func new_style_coroutine():
	print("Starting coroutine")
	yield(get_tree().create_timer(1.0), "timeout")
	print("Coroutine continued after 1 second")

# Lambda/anonymous functions (GDScript 4.0+)
func use_lambdas():
	var numbers: Array[int] = [1, 2, 3, 4, 5]

	var doubled = numbers.map(func(x): return x * 2)
	var evens = numbers.filter(func(x): return x % 2 == 0)
	var sum = numbers.reduce(func(acc, x): return acc + x, 0)

# Function overloading simulation
func attack():
	_perform_basic_attack()

func attack(target: Node2D):
	_perform_targeted_attack(target)

func attack(target: Node2D, damage: int):
	_perform_custom_attack(target, damage)

# Function with complex parameter types
func process_data(
	items: Array[Dictionary],
	config: Dictionary,
	callback: Callable = Callable()
) -> Array[String]:
	var results: Array[String] = []

	for item in items:
		if _validate_item(item, config):
			var processed: String = _process_item(item)
			results.append(processed)

			if callback.is_valid():
				callback.call(processed)

	return results

# Nested function definitions (inner functions)
func outer_function(data: Array):
	var processed_count: int = 0

	func inner_processor(item):
		processed_count += 1
		return str(item).to_upper()

	var results: Array = []
	for item in data:
		results.append(inner_processor(item))

	print("Processed %d items" % processed_count)
	return results
"#;

        let symbols = extract_symbols(gd_code);

        // Signal declarations
        let health_changed = symbols.iter().find(|s| s.name == "health_changed");
        assert!(health_changed.is_some());
        let health_changed = health_changed.unwrap();
        assert_eq!(health_changed.kind, SymbolKind::Event);
        assert!(health_changed.signature.as_ref().unwrap().contains("signal health_changed(new_health: int)"));

        let player_died = symbols.iter().find(|s| s.name == "player_died");
        assert!(player_died.is_some());
        assert_eq!(player_died.unwrap().kind, SymbolKind::Event);

        let item_collected = symbols.iter().find(|s| s.name == "item_collected");
        assert!(item_collected.is_some());
        assert!(item_collected.unwrap().signature.as_ref().unwrap().contains("signal item_collected(item_name: String, quantity: int)"));

        // Built-in lifecycle functions
        let init = symbols.iter().find(|s| s.name == "_init");
        assert!(init.is_some());
        assert_eq!(init.unwrap().kind, SymbolKind::Constructor);

        let ready = symbols.iter().find(|s| s.name == "_ready");
        assert!(ready.is_some());
        let ready = ready.unwrap();
        assert_eq!(ready.kind, SymbolKind::Method);
        assert!(ready.signature.as_ref().unwrap().contains("func _ready()"));

        let process = symbols.iter().find(|s| s.name == "_process");
        assert!(process.is_some());
        assert!(process.unwrap().signature.as_ref().unwrap().contains("func _process(delta: float)"));

        let physics_process = symbols.iter().find(|s| s.name == "_physics_process");
        assert!(physics_process.is_some());

        let input = symbols.iter().find(|s| s.name == "_input");
        assert!(input.is_some());
        assert!(input.unwrap().signature.as_ref().unwrap().contains("func _input(event: InputEvent)"));

        // Custom functions
        let simple_function = symbols.iter().find(|s| s.name == "simple_function");
        assert!(simple_function.is_some());
        assert_eq!(simple_function.unwrap().kind, SymbolKind::Function);

        let function_with_params = symbols.iter().find(|s| s.name == "function_with_params");
        assert!(function_with_params.is_some());
        assert!(function_with_params.unwrap().signature.as_ref().unwrap().contains("func function_with_params(name: String, age: int, active: bool = true)"));

        let function_with_return = symbols.iter().find(|s| s.name == "function_with_return");
        assert!(function_with_return.is_some());
        assert!(function_with_return.unwrap().signature.as_ref().unwrap().contains("-> Vector2"));

        // Static functions
        let calculate_distance = symbols.iter().find(|s| s.name == "calculate_distance");
        assert!(calculate_distance.is_some());
        assert!(calculate_distance.unwrap().signature.as_ref().unwrap().contains("static func calculate_distance"));

        // Virtual functions
        let can_drop_data = symbols.iter().find(|s| s.name == "_can_drop_data");
        assert!(can_drop_data.is_some());
        assert!(can_drop_data.unwrap().signature.as_ref().unwrap().contains("-> bool"));

        // Private functions (convention)
        let setup_connections = symbols.iter().find(|s| s.name == "_setup_connections");
        assert!(setup_connections.is_some());
        assert_eq!(setup_connections.unwrap().visibility.as_ref().unwrap(), &Visibility::Private);

        let update_movement = symbols.iter().find(|s| s.name == "_update_movement");
        assert!(update_movement.is_some());

        // Signal handlers
        let on_health_changed = symbols.iter().find(|s| s.name == "_on_health_changed");
        assert!(on_health_changed.is_some());
        assert!(on_health_changed.unwrap().signature.as_ref().unwrap().contains("func _on_health_changed(new_health: int)"));

        let on_player_died = symbols.iter().find(|s| s.name == "_on_player_died");
        assert!(on_player_died.is_some());

        let on_area_body_entered = symbols.iter().find(|s| s.name == "_on_area_2d_body_entered");
        assert!(on_area_body_entered.is_some());

        // Coroutine functions
        let fade_out = symbols.iter().find(|s| s.name == "fade_out");
        assert!(fade_out.is_some());
        assert!(fade_out.unwrap().signature.as_ref().unwrap().contains("func fade_out(duration: float = 1.0)"));

        let move_to_position = symbols.iter().find(|s| s.name == "move_to_position");
        assert!(move_to_position.is_some());

        let async_load_scene = symbols.iter().find(|s| s.name == "async_load_scene");
        assert!(async_load_scene.is_some());

        // Old style coroutine
        let new_style_coroutine = symbols.iter().find(|s| s.name == "new_style_coroutine");
        assert!(new_style_coroutine.is_some());

        // Lambda usage function
        let use_lambdas = symbols.iter().find(|s| s.name == "use_lambdas");
        assert!(use_lambdas.is_some());

        // Function overloading
        let attack_functions: Vec<_> = symbols.iter().filter(|s| s.name == "attack").collect();
        assert!(attack_functions.len() >= 1);

        // Complex parameter function
        let process_data = symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data.is_some());
        let process_data = process_data.unwrap();
        assert!(process_data.signature.as_ref().unwrap().contains("Array[Dictionary]"));
        assert!(process_data.signature.as_ref().unwrap().contains("-> Array[String]"));

        // Outer function with nested function
        let outer_function = symbols.iter().find(|s| s.name == "outer_function");
        assert!(outer_function.is_some());

        // Inner function should be detected
        let inner_processor = symbols.iter().find(|s| s.name == "inner_processor" && s.parent_id == outer_function.unwrap().parent_id);
        assert!(inner_processor.is_some());
        assert_eq!(inner_processor.unwrap().kind, SymbolKind::Function);
    }

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

        // Node references
        let player = symbols.iter().find(|s| s.name == "player");
        assert!(player.is_some());
        let player = player.unwrap();
        assert_eq!(player.metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("CharacterBody2D"));
        assert!(player.signature.as_ref().unwrap().contains("@onready var player: CharacterBody2D = $Player"));

        let ui_manager = symbols.iter().find(|s| s.name == "ui_manager");
        assert!(ui_manager.is_some());
        assert_eq!(ui_manager.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("UIManager"));

        let camera = symbols.iter().find(|s| s.name == "camera");
        assert!(camera.is_some());
        assert!(camera.unwrap().signature.as_ref().unwrap().contains("$Player/Camera2D"));

        // Resource preloading
        let player_scene = symbols.iter().find(|s| s.name == "PlayerScene");
        assert!(player_scene.is_some());
        let player_scene = player_scene.unwrap();
        assert_eq!(player_scene.kind, SymbolKind::Constant);
        assert!(player_scene.signature.as_ref().unwrap().contains("preload(\"res://scenes/Player.tscn\")"));

        // Resource loading
        let enemy_texture = symbols.iter().find(|s| s.name == "enemy_texture");
        assert!(enemy_texture.is_some());
        assert!(enemy_texture.unwrap().signature.as_ref().unwrap().contains("load(\"res://textures/enemy.png\")"));

        // Game state enum
        let game_state = symbols.iter().find(|s| s.name == "GameState");
        assert!(game_state.is_some());
        assert_eq!(game_state.unwrap().kind, SymbolKind::Enum);

        // Game state variables
        let current_state = symbols.iter().find(|s| s.name == "current_state");
        assert!(current_state.is_some());
        assert_eq!(current_state.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("GameState"));

        // Input handling
        let handle_keyboard_input = symbols.iter().find(|s| s.name == "_handle_keyboard_input");
        assert!(handle_keyboard_input.is_some());
        assert!(handle_keyboard_input.unwrap().signature.as_ref().unwrap().contains("func _handle_keyboard_input(event: InputEventKey)"));

        let unhandled_key_input = symbols.iter().find(|s| s.name == "_unhandled_key_input");
        assert!(unhandled_key_input.is_some());

        // Scene management
        let change_scene = symbols.iter().find(|s| s.name == "change_scene");
        assert!(change_scene.is_some());

        let load_scene_async = symbols.iter().find(|s| s.name == "load_scene_async");
        assert!(load_scene_async.is_some());

        // Node manipulation
        let spawn_enemy = symbols.iter().find(|s| s.name == "spawn_enemy");
        assert!(spawn_enemy.is_some());
        assert!(spawn_enemy.unwrap().signature.as_ref().unwrap().contains("func spawn_enemy(position: Vector2)"));

        let find_nodes_by_group = symbols.iter().find(|s| s.name == "find_nodes_by_group");
        assert!(find_nodes_by_group.is_some());
        assert!(find_nodes_by_group.unwrap().signature.as_ref().unwrap().contains("-> Array[Node]"));

        // Physics callbacks
        let on_area_body_entered = symbols.iter().find(|s| s.name == "_on_area_2d_body_entered");
        assert!(on_area_body_entered.is_some());

        let on_rigid_body_entered = symbols.iter().find(|s| s.name == "_on_rigid_body_2d_body_entered");
        assert!(on_rigid_body_entered.is_some());

        // Animation functions
        let animate_ui_element = symbols.iter().find(|s| s.name == "animate_ui_element");
        assert!(animate_ui_element.is_some());

        let shake_camera = symbols.iter().find(|s| s.name == "shake_camera");
        assert!(shake_camera.is_some());

        // Audio management
        let audio_manager = symbols.iter().find(|s| s.name == "audio_manager");
        assert!(audio_manager.is_some());
        assert_eq!(audio_manager.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("AudioStreamPlayer"));

        let play_sound = symbols.iter().find(|s| s.name == "play_sound");
        assert!(play_sound.is_some());

        let play_music = symbols.iter().find(|s| s.name == "play_music");
        assert!(play_music.is_some());

        // Save/Load system
        let save_file = symbols.iter().find(|s| s.name == "SAVE_FILE");
        assert!(save_file.is_some());
        assert_eq!(save_file.unwrap().kind, SymbolKind::Constant);

        let save_game = symbols.iter().find(|s| s.name == "save_game");
        assert!(save_game.is_some());

        let load_game = symbols.iter().find(|s| s.name == "load_game");
        assert!(load_game.is_some());
        assert!(load_game.unwrap().signature.as_ref().unwrap().contains("-> bool"));

        // Visual effects
        let create_explosion = symbols.iter().find(|s| s.name == "create_explosion");
        assert!(create_explosion.is_some());

        // Custom drawing
        let draw = symbols.iter().find(|s| s.name == "_draw");
        assert!(draw.is_some());

        let draw_debug_info = symbols.iter().find(|s| s.name == "_draw_debug_info");
        assert!(draw_debug_info.is_some());

        // Signal declarations
        let game_state_changed = symbols.iter().find(|s| s.name == "game_state_changed");
        assert!(game_state_changed.is_some());
        assert_eq!(game_state_changed.unwrap().kind, SymbolKind::Event);

        let score_updated = symbols.iter().find(|s| s.name == "score_updated");
        assert!(score_updated.is_some());

        // Signal handling
        let connect_signals = symbols.iter().find(|s| s.name == "_connect_signals");
        assert!(connect_signals.is_some());

        let on_game_state_changed = symbols.iter().find(|s| s.name == "_on_game_state_changed");
        assert!(on_game_state_changed.is_some());

        // Threading
        let process_large_dataset = symbols.iter().find(|s| s.name == "process_large_dataset");
        assert!(process_large_dataset.is_some());

        let background_processing = symbols.iter().find(|s| s.name == "_background_processing");
        assert!(background_processing.is_some());
        assert_eq!(background_processing.unwrap().visibility.as_ref().unwrap(), &Visibility::Private);
    }

    #[test]
    fn test_extract_annotations_type_hints_generics_and_modern_gdscript_constructs() {
        let gd_code = r#"
extends RefCounted

# Advanced type hints and generics
var typed_array: Array[String] = []
var typed_dict: Dictionary = {}
var vector_array: Array[Vector2] = []
var node_array: Array[Node] = []

# Optional and nullable types
var optional_texture: Texture2D
var nullable_node: Node
var maybe_string: String = ""

# Advanced annotations
@export_category("Player Settings")
@export var player_speed: float = 100.0

@export_group("Combat")
@export var damage: int = 10
@export var critical_chance: float = 0.1

@export_subgroup("Weapons")
@export var weapon_damage: int = 25
@export var weapon_range: float = 50.0

@export_enum("Easy", "Medium", "Hard", "Nightmare") var difficulty: int = 1
@export_flags("Fire:1", "Water:2", "Earth:4", "Air:8") var elements: int = 0

@export_file("*.json") var config_file: String
@export_dir var save_directory: String
@export_global_file("*.tscn") var scene_file: String

@export_multiline var description: String = ""
@export_placehnewer("Enter your name") var player_name: String = ""

# Custom resource types
@export var custom_resource: CustomPlayerData
@export var packed_scene: PackedScene

# Tool script functionality
@tool
extends EditorPlugin

var editor_interface: EditorInterface

func _enter_tree():
	editor_interface = get_editor_interface()
	add_custom_type(
		"CustomNode",
		"Node2D",
		preload("res://scripts/CustomNode.gd"),
		preload("res://icons/custom_node.png")
	)

func _exit_tree():
	remove_custom_type("CustomNode")

# Match statements (GDScript 4.0+)
func handle_input_action(action: String):
	match action:
		"move_left", "move_right":
			_handle_movement(action)
		"jump" when is_on_floor():
			_handle_jump()
		"attack" when can_attack:
			_handle_attack()
		var unknown_action:
			print("Unknown action: ", unknown_action)

func process_value(value):
	match typeof(value):
		TYPE_INT:
			return value * 2
		TYPE_FLOAT:
			return round(value)
		TYPE_STRING:
			return value.to_upper()
		TYPE_ARRAY:
			return value.size()
		_:
			return null

# Advanced class features
class_name AdvancedPlayer
extends CharacterBody2D

# Interface-like implementation using duck typing
interface IMovable:
	func move(direction: Vector2)
	func stop()
	func get_speed() -> float

interface IDamageable:
	func take_damage(amount: int)
	func heal(amount: int)
	func is_alive() -> bool

# Multiple "interface" implementation
extends Node2D
# implements IMovable, IDamageable  # Note: GDScript doesn't have formal interfaces

func move(direction: Vector2):
	position += direction * speed

func stop():
	velocity = Vector2.ZERO

func get_speed() -> float:
	return speed

func take_damage(amount: int):
	health -= amount

func heal(amount: int):
	health = min(health + amount, max_health)

func is_alive() -> bool:
	return health > 0

# Lambda expressions and functional programming
func use_advanced_lambdas():
	var numbers: Array[int] = range(1, 11)

	# Complex lambda with multiple operations
	var processed = numbers.map(func(x):
		var squared = x * x
		var result = squared + 10
		return result if result % 2 == 0 else 0
	)

	# Lambda with closure
	var multiplier: int = 5
	var multiplied = numbers.map(func(x): return x * multiplier)

	# Chained operations
	var result = numbers\
		.filter(func(x): return x % 2 == 0)\
		.map(func(x): return x * x)\
		.reduce(func(acc, x): return acc + x, 0)

# Async/await patterns
func complex_async_operation():
	print("Starting complex operation...")

	# Parallel async operations
	var task1 = fetch_data_async("api/users")
	var task2 = fetch_data_async("api/settings")
	var task3 = load_texture_async("res://images/background.png")

	# Wait for all to complete
	var users = await task1
	var settings = await task2
	var texture = await task3

	return {
		"users": users,
		"settings": settings,
		"texture": texture
	}

func fetch_data_async(endpoint: String):
	var http_request: HTTPRequest = HTTPRequest.new()
	add_child(http_request)

	var url = "https://api.example.com/" + endpoint
	http_request.request(url)

	var response = await http_request.request_completed
	http_request.queue_free()

	return JSON.parse_string(response[3].get_string_from_utf8())

# Custom property accessor with advanced logic
var _energy: float = 100.0
var energy: float:
	get:
		return _energy
	set(value):
		var new_energy = _energy
		_energy = clampf(value, 0.0, 100.0)

		if _energy != new_energy:
			energy_changed.emit(_energy, new_energy)

		if _energy == 0.0 and new_energy > 0.0:
			energy_depleted.emit()
		elif _energy == 100.0 and new_energy < 100.0:
			energy_full.emit()

signal energy_changed(new_value: float, new_value: float)
signal energy_depleted
signal energy_full

# Advanced generic types and type checking
func process_collection[T](items: Array[T], processor: Callable) -> Array[T]:
	var results: Array[T] = []
	for item in items:
		if processor.is_valid():
			results.append(processor.call(item))
		else:
			results.append(item)
	return results

func safe_cast[T](object: Variant, type: T) -> T:
	if object is T:
		return object as T
	else:
		return null

# Custom iterators
class NumberRange:
	var start: int
	var end: int
	var current: int

	func _init(start_val: int, end_val: int):
		start = start_val
		end = end_val
		current = start

	func _iter_init(_arg):
		current = start
		return current <= end

	func _iter_next(_arg):
		current += 1
		return current <= end

	func _iter_get(_arg):
		return current

# Usage of custom iterator
func use_custom_iterator():
	var range_obj = NumberRange.new(1, 5)
	for number in range_obj:
		print("Number: ", number)

# Advanced signal connections with parameters
func setup_advanced_connections():
	# Connect with additional parameters
	player_died.connect(_on_player_died.bind("game_over_screen"))

	# Connect to lambda
	enemy_spawned.connect(func(enemy):
		enemy.add_to_group("enemies")
		enemy.set_target(player)
	)

	# One-shot connections
	level_completed.connect(_on_level_completed, CONNECT_ONE_SHOT)

	# Deferred connections
	ui_updated.connect(_on_ui_updated, CONNECT_DEFERRED)

# Resource management and cleanup
func _notification(what: int):
	match what:
		NOTIFICATION_PREDELETE:
			_cleanup_resources()
		NOTIFICATION_WM_CLOSE_REQUEST:
			_save_before_exit()
		NOTIFICATION_APPLICATION_FOCUS_OUT:
			_pause_background_processes()

func _cleanup_resources():
	for resource in managed_resources:
		if resource.has_method("cleanup"):
			resource.cleanup()
"#;

        let symbols = extract_symbols(gd_code);

        // Advanced type hints
        let typed_array = symbols.iter().find(|s| s.name == "typed_array");
        assert!(typed_array.is_some());
        let typed_array = typed_array.unwrap();
        assert!(typed_array.signature.as_ref().unwrap().contains("var typed_array: Array[String]"));
        assert_eq!(typed_array.metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Array[String]"));

        let vector_array = symbols.iter().find(|s| s.name == "vector_array");
        assert!(vector_array.is_some());
        assert_eq!(vector_array.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Array[Vector2]"));

        // Export annotations
        let player_speed = symbols.iter().find(|s| s.name == "player_speed");
        assert!(player_speed.is_some());
        assert!(player_speed.unwrap().signature.as_ref().unwrap().contains("@export_category(\"Player Settings\")"));

        let damage = symbols.iter().find(|s| s.name == "damage");
        assert!(damage.is_some());
        assert!(damage.unwrap().signature.as_ref().unwrap().contains("@export_group(\"Combat\")"));

        let weapon_damage = symbols.iter().find(|s| s.name == "weapon_damage");
        assert!(weapon_damage.is_some());
        assert!(weapon_damage.unwrap().signature.as_ref().unwrap().contains("@export_subgroup(\"Weapons\")"));

        let difficulty = symbols.iter().find(|s| s.name == "difficulty");
        assert!(difficulty.is_some());
        assert!(difficulty.unwrap().signature.as_ref().unwrap().contains("@export_enum"));

        let elements = symbols.iter().find(|s| s.name == "elements");
        assert!(elements.is_some());
        assert!(elements.unwrap().signature.as_ref().unwrap().contains("@export_flags"));

        let config_file = symbols.iter().find(|s| s.name == "config_file");
        assert!(config_file.is_some());
        assert!(config_file.unwrap().signature.as_ref().unwrap().contains("@export_file(\"*.json\")"));

        let description = symbols.iter().find(|s| s.name == "description");
        assert!(description.is_some());
        assert!(description.unwrap().signature.as_ref().unwrap().contains("@export_multiline"));

        // Tool script
        let editor_interface = symbols.iter().find(|s| s.name == "editor_interface");
        assert!(editor_interface.is_some());

        let enter_tree = symbols.iter().find(|s| s.name == "_enter_tree");
        assert!(enter_tree.is_some());

        let exit_tree = symbols.iter().find(|s| s.name == "_exit_tree");
        assert!(exit_tree.is_some());

        // Match statements
        let handle_input_action = symbols.iter().find(|s| s.name == "handle_input_action");
        assert!(handle_input_action.is_some());

        let process_value = symbols.iter().find(|s| s.name == "process_value");
        assert!(process_value.is_some());

        // Advanced class
        let advanced_player = symbols.iter().find(|s| s.name == "AdvancedPlayer");
        assert!(advanced_player.is_some());
        assert_eq!(advanced_player.unwrap().kind, SymbolKind::Class);

        // Interface-like methods
        let move_func = symbols.iter().find(|s| s.name == "move");
        assert!(move_func.is_some());
        assert!(move_func.unwrap().signature.as_ref().unwrap().contains("func move(direction: Vector2)"));

        let take_damage = symbols.iter().find(|s| s.name == "take_damage");
        assert!(take_damage.is_some());

        let is_alive = symbols.iter().find(|s| s.name == "is_alive");
        assert!(is_alive.is_some());
        assert!(is_alive.unwrap().signature.as_ref().unwrap().contains("-> bool"));

        // Lambda usage
        let use_advanced_lambdas = symbols.iter().find(|s| s.name == "use_advanced_lambdas");
        assert!(use_advanced_lambdas.is_some());

        // Async operations
        let complex_async_operation = symbols.iter().find(|s| s.name == "complex_async_operation");
        assert!(complex_async_operation.is_some());

        let fetch_data_async = symbols.iter().find(|s| s.name == "fetch_data_async");
        assert!(fetch_data_async.is_some());

        // Advanced property with getter/setter
        let energy = symbols.iter().find(|s| s.name == "energy");
        assert!(energy.is_some());
        assert!(energy.unwrap().signature.as_ref().unwrap().contains("var energy: float:"));

        // Signals for energy
        let energy_changed = symbols.iter().find(|s| s.name == "energy_changed");
        assert!(energy_changed.is_some());
        assert_eq!(energy_changed.unwrap().kind, SymbolKind::Event);

        let energy_depleted = symbols.iter().find(|s| s.name == "energy_depleted");
        assert!(energy_depleted.is_some());

        // Generic functions
        let process_collection = symbols.iter().find(|s| s.name == "process_collection");
        assert!(process_collection.is_some());
        assert!(process_collection.unwrap().signature.as_ref().unwrap().contains("func process_collection[T]"));

        let safe_cast = symbols.iter().find(|s| s.name == "safe_cast");
        assert!(safe_cast.is_some());
        assert!(safe_cast.unwrap().signature.as_ref().unwrap().contains("func safe_cast[T]"));

        // Custom iterator class
        let number_range = symbols.iter().find(|s| s.name == "NumberRange");
        assert!(number_range.is_some());
        assert_eq!(number_range.unwrap().kind, SymbolKind::Class);

        // Iterator methods
        let iter_init = symbols.iter().find(|s| s.name == "_iter_init" && s.parent_id == number_range.unwrap().parent_id);
        assert!(iter_init.is_some());

        let iter_next = symbols.iter().find(|s| s.name == "_iter_next" && s.parent_id == number_range.unwrap().parent_id);
        assert!(iter_next.is_some());

        let iter_get = symbols.iter().find(|s| s.name == "_iter_get" && s.parent_id == number_range.unwrap().parent_id);
        assert!(iter_get.is_some());

        // Iterator usage
        let use_custom_iterator = symbols.iter().find(|s| s.name == "use_custom_iterator");
        assert!(use_custom_iterator.is_some());

        // Advanced connections
        let setup_advanced_connections = symbols.iter().find(|s| s.name == "setup_advanced_connections");
        assert!(setup_advanced_connections.is_some());

        // Notification handling
        let notification = symbols.iter().find(|s| s.name == "_notification");
        assert!(notification.is_some());
        assert!(notification.unwrap().signature.as_ref().unwrap().contains("func _notification(what: int)"));

        let cleanup_resources = symbols.iter().find(|s| s.name == "_cleanup_resources");
        assert!(cleanup_resources.is_some());
    }

    #[test]
    fn test_extract_resource_handling_custom_resources_and_serialization_patterns() {
        let gd_code = r#"
extends Resource
class_name GameData

# Custom resource properties
@export var version: String = "1.0"
@export var player_data: PlayerData
@export var world_data: WorldData
@export var settings: GameSettings

# Resource arrays
@export var levels: Array[LevelData] = []
@export var items: Array[ItemData] = []
@export var achievements: Array[AchievementData] = []

# Resource serialization
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

	# Deserialize arrays
	levels.clear()
	for level_data in data.get("levels", []):
		var level = LevelData.new()
		level.deserialize(level_data)
		levels.append(level)

# Custom PlayerData resource
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

# Custom serialization with validation
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

# Resource manager singleton
extends Node

var loaded_resources: Dictionary = {}
var resource_cache: Dictionary = {}
var loading_queue: Array = []
var max_cache_size: int = 100

signal resource_loaded(path: String, resource: Resource)
signal resource_failed(path: String, error: String)

func load_resource_async(path: String, type_hint: String = "") -> Resource:
	# Check cache first
	if resource_cache.has(path):
		return resource_cache[path]

	# Check if already loading
	if path in loading_queue:
		while path in loading_queue:
			await get_tree().process_frame
		return resource_cache.get(path)

	loading_queue.append(path)

	# Start threaded loading
	ResourceLoader.load_threaded_request(path, type_hint)

	var progress: Array = []
	while true:
		var status = ResourceLoader.load_threaded_get_status(path, progress)

		match status:
			ResourceLoader.THREAD_LOAD_LOADED:
				var resource = ResourceLoader.load_threaded_get(path)
				_cache_resource(path, resource)
				loading_queue.erase(path)
				resource_loaded.emit(path, resource)
				return resource

			ResourceLoader.THREAD_LOAD_FAILED:
				loading_queue.erase(path)
				resource_failed.emit(path, "Failed to load resource")
				return null

			ResourceLoader.THREAD_LOAD_INVALID_RESOURCE:
				loading_queue.erase(path)
				resource_failed.emit(path, "Invalid resource")
				return null

		await get_tree().process_frame

func _cache_resource(path: String, resource: Resource):
	if resource_cache.size() >= max_cache_size:
		_evict_newest_resource()

	resource_cache[path] = resource
	loaded_resources[path] = Time.get_unix_time_from_system()

func _evict_newest_resource():
	var newest_path: String = ""
	var newest_time: float = INF

	for path in loaded_resources:
		var load_time = loaded_resources[path]
		if load_time < newest_time:
			newest_time = load_time
			newest_path = path

	if newest_path:
		resource_cache.erase(newest_path)
		loaded_resources.erase(newest_path)

# Configuration resource
class_name GameConfig
extends Resource

# Graphics settings
@export_group("Graphics")
@export var resolution: Vector2i = Vector2i(1920, 1080)
@export var fullscreen: bool = false
@export var vsync: bool = true
@export_range(0.5, 2.0) var render_scale: float = 1.0
@export_enum("Low", "Medium", "High", "Ultra") var quality_preset: int = 2

# Audio settings
@export_group("Audio")
@export_range(0.0, 1.0) var master_volume: float = 1.0
@export_range(0.0, 1.0) var music_volume: float = 0.8
@export_range(0.0, 1.0) var sfx_volume: float = 1.0
@export_range(0.0, 1.0) var voice_volume: float = 1.0

# Input settings
@export_group("Input")
@export var mouse_sensitivity: float = 1.0
@export var invert_y_axis: bool = false
@export var key_bindings: Dictionary = {}

# Gameplay settings
@export_group("Gameplay")
@export var difficulty: int = 1
@export var auto_save: bool = true
@export var save_interval: float = 300.0  # 5 minutes
@export var show_hints: bool = true

func apply_settings():
	_apply_graphics_settings()
	_apply_audio_settings()
	_apply_input_settings()

func _apply_graphics_settings():
	var window = get_window()

	if fullscreen:
		window.mode = Window.MODE_FULLSCREEN
	else:
		window.mode = Window.MODE_WINDOWED
		window.size = resolution

	match vsync:
		true:
			DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_ENABLED)
		false:
			DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)

	get_viewport().scaling_3d_scale = render_scale

func _apply_audio_settings():
	var master_bus = AudioServer.get_bus_index("Master")
	var music_bus = AudioServer.get_bus_index("Music")
	var sfx_bus = AudioServer.get_bus_index("SFX")

	AudioServer.set_bus_volume_db(master_bus, linear_to_db(master_volume))
	AudioServer.set_bus_volume_db(music_bus, linear_to_db(music_volume))
	AudioServer.set_bus_volume_db(sfx_bus, linear_to_db(sfx_volume))

func save_to_file(path: String = "user://settings.cfg"):
	var config = ConfigFile.new()

	config.set_value("graphics", "resolution", resolution)
	config.set_value("graphics", "fullscreen", fullscreen)
	config.set_value("graphics", "vsync", vsync)
	config.set_value("graphics", "render_scale", render_scale)
	config.set_value("graphics", "quality_preset", quality_preset)

	config.set_value("audio", "master_volume", master_volume)
	config.set_value("audio", "music_volume", music_volume)
	config.set_value("audio", "sfx_volume", sfx_volume)
	config.set_value("audio", "voice_volume", voice_volume)

	config.set_value("input", "mouse_sensitivity", mouse_sensitivity)
	config.set_value("input", "invert_y_axis", invert_y_axis)
	config.set_value("input", "key_bindings", key_bindings)

	config.set_value("gameplay", "difficulty", difficulty)
	config.set_value("gameplay", "auto_save", auto_save)
	config.set_value("gameplay", "save_interval", save_interval)
	config.set_value("gameplay", "show_hints", show_hints)

	var error = config.save(path)
	if error != OK:
		print("Failed to save settings: ", error)

func load_from_file(path: String = "user://settings.cfg"):
	var config = ConfigFile.new()
	var error = config.load(path)

	if error != OK:
		print("Failed to load settings, using defaults")
		return

	resolution = config.get_value("graphics", "resolution", Vector2i(1920, 1080))
	fullscreen = config.get_value("graphics", "fullscreen", false)
	vsync = config.get_value("graphics", "vsync", true)
	render_scale = config.get_value("graphics", "render_scale", 1.0)
	quality_preset = config.get_value("graphics", "quality_preset", 2)

	master_volume = config.get_value("audio", "master_volume", 1.0)
	music_volume = config.get_value("audio", "music_volume", 0.8)
	sfx_volume = config.get_value("audio", "sfx_volume", 1.0)
	voice_volume = config.get_value("audio", "voice_volume", 1.0)

	mouse_sensitivity = config.get_value("input", "mouse_sensitivity", 1.0)
	invert_y_axis = config.get_value("input", "invert_y_axis", false)
	key_bindings = config.get_value("input", "key_bindings", {})

	difficulty = config.get_value("gameplay", "difficulty", 1)
	auto_save = config.get_value("gameplay", "auto_save", true)
	save_interval = config.get_value("gameplay", "save_interval", 300.0)
	show_hints = config.get_value("gameplay", "show_hints", true)
"#;

        let symbols = extract_symbols(gd_code);

        // GameData resource class
        let game_data = symbols.iter().find(|s| s.name == "GameData");
        assert!(game_data.is_some());
        let game_data = game_data.unwrap();
        assert_eq!(game_data.kind, SymbolKind::Class);
        assert_eq!(game_data.metadata.as_ref().and_then(|m| m.get("baseClass").map(|v| v.as_str().unwrap())), Some("Resource"));

        // Resource properties
        let version = symbols.iter().find(|s| s.name == "version");
        assert!(version.is_some());
        assert!(version.unwrap().signature.as_ref().unwrap().contains("@export var version: String = \"1.0\""));

        let player_data = symbols.iter().find(|s| s.name == "player_data");
        assert!(player_data.is_some());
        assert_eq!(player_data.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("PlayerData"));

        let levels = symbols.iter().find(|s| s.name == "levels");
        assert!(levels.is_some());
        assert_eq!(levels.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Array[LevelData]"));

        // Serialization methods
        let serialize = symbols.iter().find(|s| s.name == "serialize" && s.parent_id == game_data.parent_id);
        assert!(serialize.is_some());
        let serialize = serialize.unwrap();
        assert_eq!(serialize.kind, SymbolKind::Method);
        assert!(serialize.signature.as_ref().unwrap().contains("-> Dictionary"));

        let deserialize = symbols.iter().find(|s| s.name == "deserialize" && s.parent_id == game_data.parent_id);
        assert!(deserialize.is_some());

        // PlayerData class
        let player_data_class = symbols.iter().find(|s| s.name == "PlayerData");
        assert!(player_data_class.is_some());
        assert_eq!(player_data_class.unwrap().kind, SymbolKind::Class);

        // PlayerData properties
        let name = symbols.iter().find(|s| s.name == "name" && s.parent_id == player_data_class.unwrap().parent_id);
        assert!(name.is_some());
        assert_eq!(name.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("String"));

        let level = symbols.iter().find(|s| s.name == "level" && s.parent_id == player_data_class.unwrap().parent_id);
        assert!(level.is_some());
        assert_eq!(level.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("int"));

        let position = symbols.iter().find(|s| s.name == "position" && s.parent_id == player_data_class.unwrap().parent_id);
        assert!(position.is_some());
        assert_eq!(position.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Vector3"));

        let unlocked_abilities = symbols.iter().find(|s| s.name == "unlocked_abilities");
        assert!(unlocked_abilities.is_some());
        assert_eq!(unlocked_abilities.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Array[String]"));

        // Resource manager properties
        let loaded_resources = symbols.iter().find(|s| s.name == "loaded_resources");
        assert!(loaded_resources.is_some());
        assert_eq!(loaded_resources.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Dictionary"));

        let resource_cache = symbols.iter().find(|s| s.name == "resource_cache");
        assert!(resource_cache.is_some());

        let loading_queue = symbols.iter().find(|s| s.name == "loading_queue");
        assert!(loading_queue.is_some());
        assert_eq!(loading_queue.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Array"));

        // Resource manager signals
        let resource_loaded = symbols.iter().find(|s| s.name == "resource_loaded");
        assert!(resource_loaded.is_some());
        let resource_loaded = resource_loaded.unwrap();
        assert_eq!(resource_loaded.kind, SymbolKind::Event);
        assert!(resource_loaded.signature.as_ref().unwrap().contains("signal resource_loaded(path: String, resource: Resource)"));

        let resource_failed = symbols.iter().find(|s| s.name == "resource_failed");
        assert!(resource_failed.is_some());

        // Resource manager methods
        let load_resource_async = symbols.iter().find(|s| s.name == "load_resource_async");
        assert!(load_resource_async.is_some());
        assert!(load_resource_async.unwrap().signature.as_ref().unwrap().contains("-> Resource"));

        let cache_resource = symbols.iter().find(|s| s.name == "_cache_resource");
        assert!(cache_resource.is_some());
        assert_eq!(cache_resource.unwrap().visibility.as_ref().unwrap(), &Visibility::Private);

        let evict_oldest_resource = symbols.iter().find(|s| s.name == "_evict_newest_resource");
        assert!(evict_oldest_resource.is_some());

        // GameConfig class
        let game_config = symbols.iter().find(|s| s.name == "GameConfig");
        assert!(game_config.is_some());
        assert_eq!(game_config.unwrap().kind, SymbolKind::Class);

        // Graphics settings
        let resolution = symbols.iter().find(|s| s.name == "resolution" && s.parent_id == game_config.unwrap().parent_id);
        assert!(resolution.is_some());
        assert_eq!(resolution.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Vector2i"));

        let fullscreen = symbols.iter().find(|s| s.name == "fullscreen" && s.parent_id == game_config.unwrap().parent_id);
        assert!(fullscreen.is_some());
        assert_eq!(fullscreen.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("bool"));

        let render_scale = symbols.iter().find(|s| s.name == "render_scale");
        assert!(render_scale.is_some());
        assert!(render_scale.unwrap().signature.as_ref().unwrap().contains("@export_range(0.5, 2.0)"));

        let quality_preset = symbols.iter().find(|s| s.name == "quality_preset");
        assert!(quality_preset.is_some());
        assert!(quality_preset.unwrap().signature.as_ref().unwrap().contains("@export_enum"));

        // Audio settings
        let master_volume = symbols.iter().find(|s| s.name == "master_volume");
        assert!(master_volume.is_some());
        assert!(master_volume.unwrap().signature.as_ref().unwrap().contains("@export_range(0.0, 1.0)"));

        // Input settings
        let mouse_sensitivity = symbols.iter().find(|s| s.name == "mouse_sensitivity");
        assert!(mouse_sensitivity.is_some());

        let key_bindings = symbols.iter().find(|s| s.name == "key_bindings");
        assert!(key_bindings.is_some());
        assert_eq!(key_bindings.unwrap().metadata.as_ref().and_then(|m| m.get("dataType").map(|v| v.as_str().unwrap())), Some("Dictionary"));

        // Config methods
        let apply_settings = symbols.iter().find(|s| s.name == "apply_settings");
        assert!(apply_settings.is_some());

        let apply_graphics_settings = symbols.iter().find(|s| s.name == "_apply_graphics_settings");
        assert!(apply_graphics_settings.is_some());
        assert_eq!(apply_graphics_settings.unwrap().visibility.as_ref().unwrap(), &Visibility::Private);

        let apply_audio_settings = symbols.iter().find(|s| s.name == "_apply_audio_settings");
        assert!(apply_audio_settings.is_some());

        let save_to_file = symbols.iter().find(|s| s.name == "save_to_file");
        assert!(save_to_file.is_some());

        let load_from_file = symbols.iter().find(|s| s.name == "load_from_file");
        assert!(load_from_file.is_some());
    }
}

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_annotations_type_hints_generics_and_modern_gdscript_constructs() {
        let gd_code = r#"
extends RefCounted

var typed_array: Array[String] = []
var typed_dict: Dictionary = {}
var vector_array: Array[Vector2] = []
var node_array: Array[Node] = []

var optional_texture: Texture2D
var nullable_node: Node
var maybe_string: String = ""

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

@export var custom_resource: CustomPlayerData
@export var packed_scene: PackedScene

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

class_name AdvancedPlayer
extends CharacterBody2D

interface IMovable:
	func move(direction: Vector2)
	func stop()
	func get_speed() -> float

interface IDamageable:
	func take_damage(amount: int)
	func heal(amount: int)
	func is_alive() -> bool

extends Node2D

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

func use_advanced_lambdas():
	var numbers: Array[int] = range(1, 11)
	var processed = numbers.map(func(x):
		var squared = x * x
		var doubled = squared * 2
		return doubled + (x * 3)
	)

	var filtered = processed.filter(func(value):
		return value % 2 == 0 and value > 50
	)

	var reduced = filtered.reduce(func(acc, value):
		return acc + value
	, 0)

	return reduced

func complex_async_operation(url: String):
	var response = await HTTPRequest.new().request(url)
	if response.result != OK:
		push_error("Request failed")
		return null

	var parse_result = await JSON.parse_async(response.body)
	if parse_result.error != OK:
		push_error("JSON parse failed")
		return null

	return parse_result.result

func fetch_data_async(url: String) -> Dictionary:
	return await complex_async_operation(url)

var energy: float:
	set(value):
		energy = clamp(value, 0.0, 100.0)
		energy_changed.emit(energy)
		if energy <= 0.0:
			energy_depleted.emit()
	get:
		return energy

signal energy_changed(new_energy: float)
signal energy_depleted()

func process_collection[T](items: Array[T], transformer: Callable) -> Array:
	var result: Array = []
	for item in items:
		result.append(transformer.call(item))
	return result

func safe_cast[T](value, default_value: T) -> T:
	return value as T if value is T else default_value

class NumberRange:
	var current: int
	var stop: int
	var step: int

	func _init(start: int, stop: int, step: int = 1):
		self.current = start
		self.stop = stop
		self.step = step

	func _iter_init(arg):
		return self

	func _iter_next(arg):
		current += step
		if current > stop:
			return null
		return current

	func _iter_get(arg):
		return current

func use_custom_iterator():
	for value in NumberRange.new(0, 10, 2):
		print(value)

func setup_advanced_connections():
	for signal_name in ["energy_changed", "energy_depleted"]:
		connect(signal_name, Callable(self, "_on_energy_signal"))

func _on_energy_signal(signal_name: String):
	print("Energy signal received: ", signal_name)

func _notification(what: int):
	match what:
		NOTIFICATION_ENTER_TREE:
			_setup_resources()
		NOTIFICATION_EXIT_TREE:
			_cleanup_resources()

func _cleanup_resources():
	if custom_resource:
		custom_resource.release()
"#;

        let symbols = extract_symbols(gd_code);

        let player_speed = symbols.iter().find(|s| s.name == "player_speed");
        assert!(player_speed.is_some());
        assert!(player_speed
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@export_category"));

        let difficulty = symbols.iter().find(|s| s.name == "difficulty");
        assert!(difficulty.is_some());
        assert!(difficulty
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@export_enum"));

        let config_file = symbols.iter().find(|s| s.name == "config_file");
        assert!(config_file.is_some());
        assert!(config_file
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@export_file"));

        let description = symbols.iter().find(|s| s.name == "description");
        assert!(description.is_some());

        let editor_interface = symbols.iter().find(|s| s.name == "editor_interface");
        assert!(editor_interface.is_some());

        let enter_tree = symbols.iter().find(|s| s.name == "_enter_tree");
        assert!(enter_tree.is_some());
        let exit_tree = symbols.iter().find(|s| s.name == "_exit_tree");
        assert!(exit_tree.is_some());

        let handle_input_action = symbols.iter().find(|s| s.name == "handle_input_action");
        assert!(handle_input_action.is_some());

        let process_value = symbols.iter().find(|s| s.name == "process_value");
        assert!(process_value.is_some());

        let advanced_player = symbols.iter().find(|s| s.name == "AdvancedPlayer");
        assert!(advanced_player.is_some());
        assert_eq!(advanced_player.unwrap().kind, SymbolKind::Class);

        let move_func = symbols.iter().find(|s| s.name == "move");
        assert!(move_func.is_some());

        let take_damage = symbols.iter().find(|s| s.name == "take_damage");
        assert!(take_damage.is_some());

        let use_advanced_lambdas = symbols.iter().find(|s| s.name == "use_advanced_lambdas");
        assert!(use_advanced_lambdas.is_some());

        let complex_async_operation = symbols.iter().find(|s| s.name == "complex_async_operation");
        assert!(complex_async_operation.is_some());

        let fetch_data_async = symbols.iter().find(|s| s.name == "fetch_data_async");
        assert!(fetch_data_async.is_some());

        let energy = symbols.iter().find(|s| s.name == "energy");
        assert!(energy.is_some());
        assert!(energy
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("var energy: float:"));

        let energy_changed = symbols.iter().find(|s| s.name == "energy_changed");
        assert!(energy_changed.is_some());
        assert_eq!(energy_changed.unwrap().kind, SymbolKind::Event);

        let process_collection = symbols.iter().find(|s| s.name == "process_collection");
        assert!(process_collection.is_some());

        let safe_cast = symbols.iter().find(|s| s.name == "safe_cast");
        assert!(safe_cast.is_some());

        let number_range = symbols.iter().find(|s| s.name == "NumberRange");
        assert!(number_range.is_some());
        assert_eq!(number_range.unwrap().kind, SymbolKind::Class);

        let use_custom_iterator = symbols.iter().find(|s| s.name == "use_custom_iterator");
        assert!(use_custom_iterator.is_some());

        let setup_advanced_connections = symbols
            .iter()
            .find(|s| s.name == "setup_advanced_connections");
        assert!(setup_advanced_connections.is_some());

        let notification = symbols.iter().find(|s| s.name == "_notification");
        assert!(notification.is_some());

        let cleanup_resources = symbols.iter().find(|s| s.name == "_cleanup_resources");
        assert!(cleanup_resources.is_some());
    }
}

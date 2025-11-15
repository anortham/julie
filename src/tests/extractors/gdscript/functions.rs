use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_definitions_callbacks_and_signals() {
        let gd_code = r#"
extends Node2D

signal health_changed(new_health: int)
signal player_died
signal item_collected(item_name: String, quantity: int)
signal level_completed(score: int, time: float)

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

static func calculate_distance(a: Vector2, b: Vector2) -> float:
	return a.distance_to(b)

static func create_random_color() -> Color:
	return Color(randf(), randf(), randf())

func _can_drop_data(position: Vector2, data) -> bool:
	return data is Dictionary and data.has("item_type")

func _drop_data(position: Vector2, data):
	if data.has("item_type"):
		_spawn_item(data.item_type, position)

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

func fade_out(duration: float = 1.0):
	var tween: Tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, duration)
	await tween.finished

func move_to_position(target: Vector2, duration: float = 2.0):
	var tween: Tween = create_tween()
	tween.tween_property(self, "global_position", target, duration)
	await tween.finished

func async_load_scene(path: String):
	ResourceLoader.load_threaded_request(path)
	while ResourceLoader.load_threaded_get_status(path) != ResourceLoader.THREAD_LOAD_LOADED:
		await get_tree().process_frame
	return ResourceLoader.load_threaded_get(path)

func new_style_coroutine():
	print("Starting coroutine")
	yield(get_tree().create_timer(1.0), "timeout")
	print("Coroutine continued after 1 second")

func use_lambdas():
	var numbers: Array[int] = [1, 2, 3, 4, 5]
	var doubled = numbers.map(func(x): return x * 2)
	var evens = numbers.filter(func(x): return x % 2 == 0)
	var sum = numbers.reduce(func(acc, x): return acc + x, 0)

func attack():
	_perform_basic_attack()

func attack(target: Node2D):
	_perform_targeted_attack(target)

func attack(target: Node2D, damage: int):
	_perform_custom_attack(target, damage)

func process_data(items: Array[Dictionary], config: Dictionary, callback: Callable = Callable()) -> Array[String]:
	var results: Array[String] = []
	for item in items:
		if _validate_item(item, config):
			var processed: String = _process_item(item)
			results.append(processed)

			if callback.is_valid():
				callback.call(processed)
	return results

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

        let health_signal = symbols.iter().find(|s| s.name == "health_changed");
        assert!(health_signal.is_some());
        assert_eq!(health_signal.unwrap().kind, SymbolKind::Event);

        let ready_func = symbols.iter().find(|s| s.name == "_ready");
        assert!(ready_func.is_some());
        assert_eq!(ready_func.unwrap().kind, SymbolKind::Method);

        let physics_process = symbols.iter().find(|s| s.name == "_physics_process");
        assert!(physics_process.is_some());

        let simple_function = symbols.iter().find(|s| s.name == "simple_function");
        assert!(simple_function.is_some());

        let with_params = symbols.iter().find(|s| s.name == "function_with_params");
        assert!(with_params.is_some());
        assert!(
            with_params
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("active: bool = true")
        );

        let complex_return = symbols
            .iter()
            .find(|s| s.name == "function_with_complex_return");
        assert!(complex_return.is_some());
        assert!(
            complex_return
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("-> Dictionary")
        );

        let calculate_distance = symbols.iter().find(|s| s.name == "calculate_distance");
        assert!(calculate_distance.is_some());
        assert!(
            calculate_distance
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static func calculate_distance")
        );

        let can_drop_data = symbols.iter().find(|s| s.name == "_can_drop_data");
        assert!(can_drop_data.is_some());

        let setup_connections = symbols.iter().find(|s| s.name == "_setup_connections");
        assert!(setup_connections.is_some());

        let coroutine = symbols.iter().find(|s| s.name == "async_load_scene");
        assert!(coroutine.is_some());

        let new_style_coroutine = symbols.iter().find(|s| s.name == "new_style_coroutine");
        assert!(new_style_coroutine.is_some());

        let lambda_function = symbols.iter().find(|s| s.name == "use_lambdas");
        assert!(lambda_function.is_some());

        let overloaded_attack = symbols
            .iter()
            .filter(|s| s.name == "attack" && s.kind == SymbolKind::Function)
            .count();
        assert!(overloaded_attack >= 3);

        let process_data = symbols.iter().find(|s| s.name == "process_data");
        assert!(process_data.is_some());
        assert!(
            process_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Array[Dictionary]")
        );
        assert!(
            process_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("-> Array[String]")
        );

        let outer_function = symbols.iter().find(|s| s.name == "outer_function");
        assert!(outer_function.is_some());
        let outer_id = outer_function.as_ref().unwrap().id.clone();

        let inner_processor = symbols
            .iter()
            .find(|s| s.name == "inner_processor" && s.parent_id.as_deref() == Some(&outer_id));
        assert!(inner_processor.is_some());
        assert_eq!(inner_processor.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_gdscript_doc_from_function() {
        let code = r#"
## Applies damage to the player
## @param damage: Amount of damage to apply
## @return: True if player is still alive
func validate_health(damage: int) -> bool:
    return health > damage

## Handles movement input
## @param input_vector: Normalized input direction
func apply_movement(input_vector: Vector2):
    velocity = input_vector * speed
"#;

        let symbols = extract_symbols(code);

        let validate_health = symbols.iter().find(|s| s.name == "validate_health");
        assert!(validate_health.is_some());
        let validate_health = validate_health.unwrap();
        assert!(validate_health.doc_comment.is_some());
        let doc = validate_health.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Applies damage to the player"));
        assert!(doc.contains("@param damage: Amount of damage to apply"));
        assert!(doc.contains("@return: True if player is still alive"));

        let apply_movement = symbols.iter().find(|s| s.name == "apply_movement");
        assert!(apply_movement.is_some());
        let apply_movement = apply_movement.unwrap();
        assert!(apply_movement.doc_comment.is_some());
        let doc = apply_movement.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Handles movement input"));
        assert!(doc.contains("@param input_vector: Normalized input direction"));
    }
}

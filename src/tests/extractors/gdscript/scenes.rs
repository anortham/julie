use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_scene_management_and_node_operations() {
        let code = r#"
extends Node

# Scene references
@onready var player_scene = preload("res://scenes/Player.tscn")
@onready var enemy_scene = preload("res://scenes/Enemy.tscn")
@onready var bullet_scene = preload("res://scenes/Bullet.tscn")

# Node references
@onready var spawn_points = $SpawnPoints
@onready var player = $Player
@onready var camera = $Camera2D
@onready var ui_layer = $UI

# Scene management variables
var current_level: int = 1
var loaded_scenes: Dictionary = {}
var active_enemies: Array = []

func _ready():
    # Initialize scene
    setup_initial_scene()

func setup_initial_scene():
    # Spawn player
    if player_scene:
        var player_instance = player_scene.instantiate()
        add_child(player_instance)
        player = player_instance

    # Setup camera
    if camera:
        camera.follow_target = player

    # Load initial enemies
    spawn_enemies(5)

func spawn_enemies(count: int):
    for i in range(count):
        if enemy_scene:
            var enemy_instance = enemy_scene.instantiate()
            var spawn_point = get_random_spawn_point()

            if spawn_point:
                enemy_instance.position = spawn_point.global_position
                add_child(enemy_instance)
                active_enemies.append(enemy_instance)

                # Connect enemy signals
                enemy_instance.connect("died", Callable(self, "_on_enemy_died"))

func get_random_spawn_point() -> Node2D:
    if spawn_points and spawn_points.get_child_count() > 0:
        var random_index = randi() % spawn_points.get_child_count()
        return spawn_points.get_child(random_index)
    return null

func change_level(new_level: int):
    # Save current level state
    save_level_state()

    # Unload current level
    unload_current_level()

    # Load new level
    current_level = new_level
    load_level(new_level)

func load_level(level_number: int):
    var level_path = "res://scenes/levels/Level%d.tscn" % level_number

    if ResourceLoader.exists(level_path):
        var level_scene = load(level_path)
        var level_instance = level_scene.instantiate()

        add_child(level_instance)
        loaded_scenes[level_number] = level_instance

        # Setup level-specific elements
        setup_level_elements(level_instance)

func unload_current_level():
    if loaded_scenes.has(current_level):
        var level_instance = loaded_scenes[current_level]
        level_instance.queue_free()
        loaded_scenes.erase(current_level)

    # Clear enemies
    for enemy in active_enemies:
        if is_instance_valid(enemy):
            enemy.queue_free()
    active_enemies.clear()

func save_level_state():
    # Save player position, inventory, etc.
    var save_data = {
        "player_position": player.global_position if player else Vector2.ZERO,
        "current_health": player.health if player else 100,
        "inventory": player.inventory if player else [],
        "active_enemies": active_enemies.size()
    }

    # Save to file or global state
    GameState.save_level_data(current_level, save_data)

func setup_level_elements(level_instance: Node):
    # Find and setup spawn points
    spawn_points = level_instance.get_node_or_null("SpawnPoints")

    # Find and setup objectives
    var objectives = level_instance.get_node_or_null("Objectives")
    if objectives:
        for objective in objectives.get_children():
            objective.connect("completed", Callable(self, "_on_objective_completed"))

func _on_enemy_died(enemy: Node):
    active_enemies.erase(enemy)

    # Check win condition
    if active_enemies.is_empty():
        level_completed()

func level_completed():
    print("Level %d completed!" % current_level)

    # Show completion UI
    if ui_layer:
        var completion_screen = preload("res://scenes/ui/LevelComplete.tscn").instantiate()
        ui_layer.add_child(completion_screen)

    # Auto-advance to next level after delay
    await get_tree().create_timer(3.0).timeout
    change_level(current_level + 1)

func _on_objective_completed(objective: Node):
    print("Objective completed: %s" % objective.name)

    # Update UI, play sound, etc.
    if ui_layer:
        ui_layer.update_objective_progress()
"#;

        let symbols = extract_symbols(code);

        // Test scene references
        let player_scene = symbols.iter().find(|s| s.name == "player_scene");
        assert!(player_scene.is_some());
        assert_eq!(player_scene.unwrap().kind, SymbolKind::Field);

        let enemy_scene = symbols.iter().find(|s| s.name == "enemy_scene");
        assert!(enemy_scene.is_some());

        let bullet_scene = symbols.iter().find(|s| s.name == "bullet_scene");
        assert!(bullet_scene.is_some());

        // Test node references
        let spawn_points = symbols.iter().find(|s| s.name == "spawn_points");
        assert!(spawn_points.is_some());

        let player = symbols.iter().find(|s| s.name == "player");
        assert!(player.is_some());

        let camera = symbols.iter().find(|s| s.name == "camera");
        assert!(camera.is_some());

        let ui_layer = symbols.iter().find(|s| s.name == "ui_layer");
        assert!(ui_layer.is_some());

        // Test scene management variables
        let current_level = symbols.iter().find(|s| s.name == "current_level");
        assert!(current_level.is_some());

        let loaded_scenes = symbols.iter().find(|s| s.name == "loaded_scenes");
        assert!(loaded_scenes.is_some());

        let active_enemies = symbols.iter().find(|s| s.name == "active_enemies");
        assert!(active_enemies.is_some());

        // Test functions
        let ready = symbols.iter().find(|s| s.name == "_ready");
        assert!(ready.is_some());
        assert_eq!(ready.unwrap().kind, SymbolKind::Method);

        let setup_initial_scene = symbols.iter().find(|s| s.name == "setup_initial_scene");
        assert!(setup_initial_scene.is_some());

        let spawn_enemies = symbols.iter().find(|s| s.name == "spawn_enemies");
        assert!(spawn_enemies.is_some());

        let get_random_spawn_point = symbols.iter().find(|s| s.name == "get_random_spawn_point");
        assert!(get_random_spawn_point.is_some());

        let change_level = symbols.iter().find(|s| s.name == "change_level");
        assert!(change_level.is_some());

        let load_level = symbols.iter().find(|s| s.name == "load_level");
        assert!(load_level.is_some());

        let unload_current_level = symbols.iter().find(|s| s.name == "unload_current_level");
        assert!(unload_current_level.is_some());

        let save_level_state = symbols.iter().find(|s| s.name == "save_level_state");
        assert!(save_level_state.is_some());

        let setup_level_elements = symbols.iter().find(|s| s.name == "setup_level_elements");
        assert!(setup_level_elements.is_some());

        let on_enemy_died = symbols.iter().find(|s| s.name == "_on_enemy_died");
        assert!(on_enemy_died.is_some());

        let level_completed = symbols.iter().find(|s| s.name == "level_completed");
        assert!(level_completed.is_some());

        let on_objective_completed = symbols.iter().find(|s| s.name == "_on_objective_completed");
        assert!(on_objective_completed.is_some());
    }
}

use super::*;
use crate::base::{SymbolKind, Visibility};

#[cfg(test)]
mod tests {
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

        let player = symbols.iter().find(|s| s.name == "Player");
        assert!(player.is_some());
        let player = player.unwrap();
        assert_eq!(player.kind, SymbolKind::Class);
        assert!(
            player
                .signature
                .as_ref()
                .unwrap()
                .contains("class_name Player")
        );
        assert_eq!(
            player
                .metadata
                .as_ref()
                .and_then(|m| m.get("baseClass").and_then(|v| v.as_str())),
            Some("CharacterBody2D")
        );

        let enemy = symbols.iter().find(|s| s.name == "Enemy");
        assert!(enemy.is_some());
        assert_eq!(
            enemy
                .unwrap()
                .metadata
                .as_ref()
                .and_then(|m| m.get("baseClass").and_then(|v| v.as_str())),
            Some("Actor")
        );

        let health_component = symbols.iter().find(|s| s.name == "HealthComponent");
        assert!(health_component.is_some());
        let health_component = health_component.unwrap();
        assert_eq!(health_component.kind, SymbolKind::Class);

        let take_damage = symbols.iter().find(|s| {
            s.name == "take_damage" && s.parent_id.as_deref() == Some(&health_component.id)
        });
        assert!(take_damage.is_some());
        assert_eq!(take_damage.unwrap().kind, SymbolKind::Method);
        assert!(
            take_damage
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("func take_damage(amount: int) -> bool")
        );

        let custom_resource = symbols.iter().find(|s| s.name == "CustomResource");
        assert!(custom_resource.is_some());
        assert!(
            custom_resource
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@tool")
        );

        let health = symbols.iter().find(|s| s.name == "health");
        assert!(health.is_some());
        let health = health.unwrap();
        assert_eq!(health.kind, SymbolKind::Field);
        assert_eq!(
            health
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("int")
        );
        assert!(
            health
                .signature
                .as_ref()
                .unwrap()
                .contains("var health: int = 100")
        );

        let player_name = symbols.iter().find(|s| s.name == "player_name");
        assert!(player_name.is_some());
        assert_eq!(
            player_name
                .unwrap()
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("String")
        );

        let speed = symbols.iter().find(|s| s.name == "speed");
        assert!(speed.is_some());
        let speed = speed.unwrap();
        assert!(
            speed
                .signature
                .as_ref()
                .unwrap()
                .contains("@export var speed: float = 200.0")
        );
        assert_eq!(speed.visibility.as_ref().unwrap(), &Visibility::Public);

        let armor = symbols.iter().find(|s| s.name == "armor");
        assert!(armor.is_some());
        assert!(
            armor
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@export_range(0, 100)")
        );

        let legacy_speed = symbols.iter().find(|s| {
            s.name == "legacy_speed" && s.signature.as_ref().unwrap().contains("export var")
        });
        assert!(legacy_speed.is_some());

        let sprite = symbols.iter().find(|s| s.name == "sprite");
        assert!(sprite.is_some());
        let sprite = sprite.unwrap();
        assert!(
            sprite
                .signature
                .as_ref()
                .unwrap()
                .contains("@onready var sprite: Sprite2D")
        );
        assert_eq!(
            sprite
                .metadata
                .as_ref()
                .and_then(|m| m.get("dataType").and_then(|v| v.as_str())),
            Some("Sprite2D")
        );

        let max_lives = symbols
            .iter()
            .find(|s| s.name == "MAX_LIVES" && s.kind == SymbolKind::Constant);
        assert!(max_lives.is_some());

        let state_enum = symbols
            .iter()
            .find(|s| s.name == "State" && s.kind == SymbolKind::Enum);
        assert!(state_enum.is_some());

        let network_player = symbols.iter().find(|s| {
            s.name == "NetworkPlayer"
                && s.metadata
                    .as_ref()
                    .and_then(|m| m.get("baseClass").and_then(|v| v.as_str()))
                    == Some("Player")
        });
        assert!(network_player.is_some());

        let instance_count = symbols.iter().find(|s| {
            s.name == "instance_count" && s.signature.as_ref().unwrap().contains("static var")
        });
        assert!(instance_count.is_some());

        let score_property = symbols.iter().find(|s| s.name == "_score");
        assert!(score_property.is_some());
        assert!(
            score_property
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("setget set_score, get_score")
        );

        let set_score = symbols
            .iter()
            .find(|s| s.name == "set_score" && s.kind == SymbolKind::Method);
        assert!(set_score.is_some());

        let level_property = symbols
            .iter()
            .find(|s| s.name == "level" && s.signature.as_ref().unwrap().contains("set(value)"));
        assert!(level_property.is_some());
    }

    #[test]
    fn test_extract_gdscript_doc_from_class() {
        let code = r#"
## PlayerController manages player input and movement
## Handles WASD movement and jumping
class_name PlayerController
extends Node2D

## Enemy AI controller for basic combat
class_name Enemy
extends CharacterBody2D
"#;

        let symbols = extract_symbols(code);

        let player_controller = symbols.iter().find(|s| s.name == "PlayerController");
        assert!(player_controller.is_some());
        let player_controller = player_controller.unwrap();
        assert!(player_controller.doc_comment.is_some());
        let doc = player_controller.doc_comment.as_ref().unwrap();
        assert!(doc.contains("PlayerController manages player input and movement"));
        assert!(doc.contains("Handles WASD movement and jumping"));

        let enemy = symbols.iter().find(|s| s.name == "Enemy");
        assert!(enemy.is_some());
        let enemy = enemy.unwrap();
        assert!(enemy.doc_comment.is_some());
        let doc = enemy.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Enemy AI controller for basic combat"));
    }

    #[test]
    fn test_extract_gdscript_doc_from_inner_class() {
        let code = r#"
## HealthComponent handles all health-related logic
## Supports damage, healing, and status effects
class HealthComponent:
	var max_health: int = 100
	var current_health: int

	func _init(health: int = 100):
		max_health = health
		current_health = health
"#;

        let symbols = extract_symbols(code);

        let health_component = symbols.iter().find(|s| s.name == "HealthComponent");
        assert!(health_component.is_some());
        let health_component = health_component.unwrap();
        assert!(health_component.doc_comment.is_some());
        let doc = health_component.doc_comment.as_ref().unwrap();
        assert!(doc.contains("HealthComponent handles all health-related logic"));
        assert!(doc.contains("Supports damage, healing, and status effects"));
    }
}

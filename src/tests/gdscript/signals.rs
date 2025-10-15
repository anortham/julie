use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_signals_and_signal_connections() {
        let code = r#"
extends Node

# Signal declarations
signal health_changed(new_health, old_health)
signal player_died
signal item_collected(item_name, item_type)
signal level_completed(level_number, score)

# Custom signal with parameters
signal custom_event(data: Dictionary, source: Node)

# Export variables
@export var max_health: int = 100
@export var current_health: int = 100

func _ready():
    # Connect signals
    health_changed.connect(_on_health_changed)
    player_died.connect(_on_player_died)
    item_collected.connect(_on_item_collected)

    # Connect to external signals
    $Player.connect("health_changed", Callable(self, "_on_player_health_changed"))
    get_node("../GameManager").connect("level_started", Callable(self, "_on_level_started"))

func take_damage(amount: int):
    var old_health = current_health
    current_health = max(current_health - amount, 0)

    # Emit signal
    health_changed.emit(current_health, old_health)

    if current_health <= 0:
        player_died.emit()

func collect_item(item_name: String, item_type: String):
    # Emit signal with parameters
    item_collected.emit(item_name, item_type)

func complete_level(level_number: int, score: int):
    # Emit level completion signal
    level_completed.emit(level_number, score)

# Signal handlers
func _on_health_changed(new_health: int, old_health: int):
    print("Health changed from %d to %d" % [old_health, new_health])

    # Update UI
    $HealthBar.value = new_health
    $HealthLabel.text = str(new_health) + "/" + str(max_health)

func _on_player_died():
    print("Player died!")
    # Game over logic
    get_tree().paused = true
    $GameOverScreen.show()

func _on_item_collected(item_name: String, item_type: String):
    print("Collected %s (%s)" % [item_name, item_type])

    # Add to inventory
    Inventory.add_item(item_name, item_type)

func _on_player_health_changed(new_health: int, old_health: int):
    # Handle external player health changes
    current_health = new_health

func _on_level_started():
    print("Level started!")
    # Reset level state
    current_health = max_health
    health_changed.emit(current_health, 0)
"#;

        let symbols = extract_symbols(code);

        // Test signal declarations
        let health_changed = symbols.iter().find(|s| s.name == "health_changed");
        assert!(health_changed.is_some());
        assert_eq!(health_changed.unwrap().kind, SymbolKind::Event); // Signals are events

        let player_died = symbols.iter().find(|s| s.name == "player_died");
        assert!(player_died.is_some());

        let item_collected = symbols.iter().find(|s| s.name == "item_collected");
        assert!(item_collected.is_some());

        let level_completed = symbols.iter().find(|s| s.name == "level_completed");
        assert!(level_completed.is_some());

        let custom_event = symbols.iter().find(|s| s.name == "custom_event");
        assert!(custom_event.is_some());

        // Test functions
        let ready = symbols.iter().find(|s| s.name == "_ready");
        assert!(ready.is_some());
        assert_eq!(ready.unwrap().kind, SymbolKind::Method);

        let take_damage = symbols.iter().find(|s| s.name == "take_damage");
        assert!(take_damage.is_some());

        let collect_item = symbols.iter().find(|s| s.name == "collect_item");
        assert!(collect_item.is_some());

        let complete_level = symbols.iter().find(|s| s.name == "complete_level");
        assert!(complete_level.is_some());

        // Test signal handlers
        let on_health_changed = symbols.iter().find(|s| s.name == "_on_health_changed");
        assert!(on_health_changed.is_some());

        let on_player_died = symbols.iter().find(|s| s.name == "_on_player_died");
        assert!(on_player_died.is_some());

        let on_item_collected = symbols.iter().find(|s| s.name == "_on_item_collected");
        assert!(on_item_collected.is_some());

        let on_player_health_changed = symbols.iter().find(|s| s.name == "_on_player_health_changed");
        assert!(on_player_health_changed.is_some());

        let on_level_started = symbols.iter().find(|s| s.name == "_on_level_started");
        assert!(on_level_started.is_some());
    }
}
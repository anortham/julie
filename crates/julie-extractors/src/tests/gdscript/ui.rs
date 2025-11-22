use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ui_controls_and_event_handling() {
        let code = r#"
extends Control

# UI Node references
@onready var health_bar: ProgressBar = $HealthBar
@onready var score_label: Label = $ScoreLabel
@onready var inventory_grid: GridContainer = $InventoryGrid
@onready var pause_menu: Panel = $PauseMenu
@onready var settings_dialog: Window = $SettingsDialog

# UI State variables
var is_paused: bool = false
var current_score: int = 0
var player_health: float = 100.0
var inventory_items: Array = []

func _ready():
    # Connect UI signals
    $StartButton.connect("pressed", Callable(self, "_on_start_button_pressed"))
    $PauseButton.connect("pressed", Callable(self, "_on_pause_button_pressed"))
    $SettingsButton.connect("pressed", Callable(self, "_on_settings_button_pressed"))
    $QuitButton.connect("pressed", Callable(self, "_on_quit_button_pressed"))

    # Initialize UI
    update_health_display()
    update_score_display()
    refresh_inventory_display()

func update_health_display():
    if health_bar:
        health_bar.value = player_health
        health_bar.max_value = 100.0

        # Change color based on health
        if player_health > 60:
            health_bar.modulate = Color.GREEN
        elif player_health > 30:
            health_bar.modulate = Color.YELLOW
        else:
            health_bar.modulate = Color.RED

func update_score_display():
    if score_label:
        score_label.text = "Score: %d" % current_score

        # Animate score changes
        var tween = create_tween()
        tween.tween_property(score_label, "scale", Vector2.ONE * 1.2, 0.1)
        tween.tween_property(score_label, "scale", Vector2.ONE, 0.1)

func refresh_inventory_display():
    if inventory_grid:
        # Clear existing items
        for child in inventory_grid.get_children():
            child.queue_free()

        # Add current items
        for i in range(inventory_items.size()):
            var item = inventory_items[i]
            var item_button = Button.new()
            item_button.text = item.name
            item_button.connect("pressed", Callable(self, "_on_inventory_item_pressed").bind(i))
            inventory_grid.add_child(item_button)

func show_pause_menu():
    if pause_menu:
        pause_menu.show()
        get_tree().paused = true
        is_paused = true

func hide_pause_menu():
    if pause_menu:
        pause_menu.hide()
        get_tree().paused = false
        is_paused = false

func show_settings_dialog():
    if settings_dialog:
        settings_dialog.popup_centered()

func add_inventory_item(item_name: String, item_icon: Texture2D = null):
    var item = {
        "name": item_name,
        "icon": item_icon,
        "quantity": 1
    }

    inventory_items.append(item)
    refresh_inventory_display()

    # Show pickup animation
    show_item_pickup_animation(item_name)

func remove_inventory_item(index: int):
    if index >= 0 and index < inventory_items.size():
        var removed_item = inventory_items[index]
        inventory_items.remove_at(index)
        refresh_inventory_display()

        return removed_item
    return null

func show_item_pickup_animation(item_name: String):
    var label = Label.new()
    label.text = "Picked up: " + item_name
    label.modulate = Color.GREEN
    add_child(label)

    # Animate and remove
    var tween = create_tween()
    tween.tween_property(label, "position:y", label.position.y - 50, 1.0)
    tween.tween_property(label, "modulate:a", 0.0, 1.0)
    tween.tween_callback(Callable(label, "queue_free"))

func _on_start_button_pressed():
    print("Game started!")
    # Start game logic
    get_parent().start_game()

func _on_pause_button_pressed():
    if is_paused:
        hide_pause_menu()
    else:
        show_pause_menu()

func _on_settings_button_pressed():
    show_settings_dialog()

func _on_quit_button_pressed():
    print("Quitting game...")
    get_tree().quit()

func _on_inventory_item_pressed(item_index: int):
    var item = inventory_items[item_index]
    print("Using item: %s" % item.name)

    # Use item logic
    use_inventory_item(item_index)

func use_inventory_item(item_index: int):
    var item = remove_inventory_item(item_index)
    if item:
        # Apply item effects
        match item.name:
            "Health Potion":
                player_health = min(player_health + 25, 100)
                update_health_display()
            "Score Booster":
                current_score += 100
                update_score_display()
            _:
                print("Unknown item: %s" % item.name)

# Input handling
func _input(event):
    if event.is_action_pressed("ui_cancel"):
        if is_paused:
            hide_pause_menu()
        else:
            show_pause_menu()
    elif event.is_action_pressed("inventory"):
        toggle_inventory()
    elif event.is_action_pressed("settings"):
        show_settings_dialog()

func toggle_inventory():
    var inventory_panel = $InventoryPanel
    if inventory_panel:
        if inventory_panel.visible:
            inventory_panel.hide()
        else:
            inventory_panel.show()
            refresh_inventory_display()
"#;

        let symbols = extract_symbols(code);

        // Test UI node references
        let health_bar = symbols.iter().find(|s| s.name == "health_bar");
        assert!(health_bar.is_some());
        assert_eq!(health_bar.unwrap().kind, SymbolKind::Field);

        let score_label = symbols.iter().find(|s| s.name == "score_label");
        assert!(score_label.is_some());

        let inventory_grid = symbols.iter().find(|s| s.name == "inventory_grid");
        assert!(inventory_grid.is_some());

        let pause_menu = symbols.iter().find(|s| s.name == "pause_menu");
        assert!(pause_menu.is_some());

        let settings_dialog = symbols.iter().find(|s| s.name == "settings_dialog");
        assert!(settings_dialog.is_some());

        // Test UI state variables
        let is_paused = symbols.iter().find(|s| s.name == "is_paused");
        assert!(is_paused.is_some());

        let current_score = symbols.iter().find(|s| s.name == "current_score");
        assert!(current_score.is_some());

        let player_health = symbols.iter().find(|s| s.name == "player_health");
        assert!(player_health.is_some());

        let inventory_items = symbols.iter().find(|s| s.name == "inventory_items");
        assert!(inventory_items.is_some());

        // Test UI functions
        let ready = symbols.iter().find(|s| s.name == "_ready");
        assert!(ready.is_some());
        assert_eq!(ready.unwrap().kind, SymbolKind::Method);

        let update_health_display = symbols.iter().find(|s| s.name == "update_health_display");
        assert!(update_health_display.is_some());

        let update_score_display = symbols.iter().find(|s| s.name == "update_score_display");
        assert!(update_score_display.is_some());

        let refresh_inventory_display = symbols
            .iter()
            .find(|s| s.name == "refresh_inventory_display");
        assert!(refresh_inventory_display.is_some());

        let show_pause_menu = symbols.iter().find(|s| s.name == "show_pause_menu");
        assert!(show_pause_menu.is_some());

        let hide_pause_menu = symbols.iter().find(|s| s.name == "hide_pause_menu");
        assert!(hide_pause_menu.is_some());

        let show_settings_dialog = symbols.iter().find(|s| s.name == "show_settings_dialog");
        assert!(show_settings_dialog.is_some());

        let add_inventory_item = symbols.iter().find(|s| s.name == "add_inventory_item");
        assert!(add_inventory_item.is_some());

        let remove_inventory_item = symbols.iter().find(|s| s.name == "remove_inventory_item");
        assert!(remove_inventory_item.is_some());

        let show_item_pickup_animation = symbols
            .iter()
            .find(|s| s.name == "show_item_pickup_animation");
        assert!(show_item_pickup_animation.is_some());

        // Test event handlers
        let on_start_button_pressed = symbols
            .iter()
            .find(|s| s.name == "_on_start_button_pressed");
        assert!(on_start_button_pressed.is_some());

        let on_pause_button_pressed = symbols
            .iter()
            .find(|s| s.name == "_on_pause_button_pressed");
        assert!(on_pause_button_pressed.is_some());

        let on_inventory_item_pressed = symbols
            .iter()
            .find(|s| s.name == "_on_inventory_item_pressed");
        assert!(on_inventory_item_pressed.is_some());

        let input = symbols.iter().find(|s| s.name == "_input");
        assert!(input.is_some());

        let toggle_inventory = symbols.iter().find(|s| s.name == "toggle_inventory");
        assert!(toggle_inventory.is_some());

        let use_inventory_item = symbols.iter().find(|s| s.name == "use_inventory_item");
        assert!(use_inventory_item.is_some());
    }
}

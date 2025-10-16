extends CharacterBody2D

# Player Controller Script for Godot Engine
# Demonstrates complex GDScript patterns including:
# - Class inheritance and signals
# - State management and input handling
# - Animation and physics integration
# - Resource management and exports

class_name PlayerController

# Exported variables (configurable in editor)
@export var speed: float = 300.0
@export var jump_velocity: float = -400.0
@export var max_health: int = 100
@export var dash_speed: float = 600.0
@export var dash_duration: float = 0.2

# Get the gravity from the project settings to be synced with RigidBody nodes
var gravity = ProjectSettings.get_setting("physics/2d/default_gravity")

# Player state management
enum PlayerState {
	IDLE,
	RUNNING,
	JUMPING,
	FALLING,
	DASHING,
	ATTACKING,
	HURT,
	DEAD
}

# Internal variables
var current_state: PlayerState = PlayerState.IDLE
var current_health: int
var is_dashing: bool = false
var dash_timer: float = 0.0
var can_dash: bool = true
var facing_direction: int = 1

# Node references
@onready var sprite: Sprite2D = $Sprite2D
@onready var animation_player: AnimationPlayer = $AnimationPlayer
@onready var collision_shape: CollisionShape2D = $CollisionShape2D
@onready var dash_timer_node: Timer = $DashTimer
@onready var hurt_box: Area2D = $HurtBox
@onready var hit_box: Area2D = $HitBox

# Signals for game state communication
signal health_changed(new_health: int)
signal player_died
signal item_collected(item_type: String, amount: int)
signal enemy_defeated(enemy: Node)

func _ready() -> void:
	# Initialize player state
	current_health = max_health

	# Connect signals
	dash_timer_node.timeout.connect(_on_dash_timer_timeout)
	hurt_box.body_entered.connect(_on_hurt_box_body_entered)
	hit_box.body_entered.connect(_on_hit_box_body_entered)

	# Set up physics
	floor_stop_on_slope = false
	floor_max_angle = deg_to_rad(45)

func _physics_process(delta: float) -> void:
	# Handle different states
	match current_state:
		PlayerState.DEAD:
			return
		PlayerState.HURT:
			_handle_hurt_state(delta)
		PlayerState.DASHING:
			_handle_dash_state(delta)
		_:
			_handle_normal_movement(delta)

	# Apply movement
	move_and_slide()

	# Update animations
	_update_animations()

func _handle_normal_movement(delta: float) -> void:
	# Add gravity
	if not is_on_floor():
		velocity.y += gravity * delta

	# Handle jump
	if Input.is_action_just_pressed("ui_accept") and is_on_floor():
		velocity.y = jump_velocity
		_change_state(PlayerState.JUMPING)

	# Handle dash
	if Input.is_action_just_pressed("dash") and can_dash:
		_start_dash()
		return

	# Get input direction
	var direction = Input.get_axis("ui_left", "ui_right")

	if direction != 0:
		velocity.x = direction * speed
		facing_direction = int(sign(direction))

		if is_on_floor():
			_change_state(PlayerState.RUNNING)
	else:
		velocity.x = move_toward(velocity.x, 0, speed * 3 * delta)

		if is_on_floor():
			_change_state(PlayerState.IDLE)

	# Update state based on vertical movement
	if not is_on_floor():
		if velocity.y < 0:
			_change_state(PlayerState.JUMPING)
		else:
			_change_state(PlayerState.FALLING)

func _handle_dash_state(delta: float) -> void:
	dash_timer -= delta

	if dash_timer <= 0:
		_end_dash()
	else:
		# Maintain dash velocity
		velocity.x = facing_direction * dash_speed
		velocity.y = 0  # Ignore gravity during dash

func _handle_hurt_state(delta: float) -> void:
	# Knockback and brief invincibility
	velocity.x = move_toward(velocity.x, 0, speed * 2 * delta)

	# Add gravity
	if not is_on_floor():
		velocity.y += gravity * delta

func _start_dash() -> void:
	if current_state == PlayerState.DEAD or is_dashing:
		return

	is_dashing = true
	can_dash = false
	dash_timer = dash_duration
	_change_state(PlayerState.DASHING)

	# Start cooldown timer
	dash_timer_node.start()

func _end_dash() -> void:
	is_dashing = false
	dash_timer = 0.0
	_change_state(PlayerState.IDLE)

func _on_dash_timer_timeout() -> void:
	can_dash = true

func _change_state(new_state: PlayerState) -> void:
	if current_state == new_state or current_state == PlayerState.DEAD:
		return

	# Exit current state
	match current_state:
		PlayerState.ATTACKING:
			hit_box.monitoring = false

	# Enter new state
	current_state = new_state

	match new_state:
		PlayerState.ATTACKING:
			hit_box.monitoring = true
			velocity.x *= 0.5  # Slow down during attack

func _update_animations() -> void:
	# Update sprite facing direction
	sprite.flip_h = facing_direction < 0

	# Play appropriate animation
	match current_state:
		PlayerState.IDLE:
			animation_player.play("idle")
		PlayerState.RUNNING:
			animation_player.play("run")
		PlayerState.JUMPING:
			animation_player.play("jump")
		PlayerState.FALLING:
			animation_player.play("fall")
		PlayerState.DASHING:
			animation_player.play("dash")
		PlayerState.ATTACKING:
			animation_player.play("attack")
		PlayerState.HURT:
			animation_player.play("hurt")
		PlayerState.DEAD:
			animation_player.play("death")

func _on_hurt_box_body_entered(body: Node2D) -> void:
	if body.is_in_group("enemies"):
		take_damage(body.damage_amount if body.has_method("get_damage") else 10)
	elif body.is_in_group("hazards"):
		take_damage(25)

func _on_hit_box_body_entered(body: Node2D) -> void:
	if body.is_in_group("enemies") and current_state == PlayerState.ATTACKING:
		body.take_damage(20)
		enemy_defeated.emit(body)

func take_damage(amount: int) -> void:
	if current_state == PlayerState.DEAD or current_state == PlayerState.HURT:
		return

	current_health -= amount
	current_health = max(0, current_health)

	health_changed.emit(current_health)

	if current_health <= 0:
		die()
	else:
		_change_state(PlayerState.HURT)
		# Brief invincibility
		var tween = create_tween()
		tween.tween_method(_set_modulate_alpha, 1.0, 0.3, 0.1)
		tween.tween_method(_set_modulate_alpha, 0.3, 1.0, 0.1)
		tween.tween_callback(_change_state.bind(PlayerState.IDLE))

func heal(amount: int) -> void:
	current_health += amount
	current_health = min(max_health, current_health)
	health_changed.emit(current_health)

func die() -> void:
	_change_state(PlayerState.DEAD)
	collision_shape.disabled = true
	player_died.emit()

func _set_modulate_alpha(alpha: float) -> void:
	modulate.a = alpha

func collect_item(item_type: String, amount: int = 1) -> void:
	match item_type:
		"health_potion":
			heal(25)
		"coin":
			pass  # Handle in game manager
		"power_up":
			speed *= 1.2
			jump_velocity *= 1.1

	item_collected.emit(item_type, amount)

# Attack input handling
func _input(event: InputEvent) -> void:
	if event.is_action_pressed("attack"):
		if current_state in [PlayerState.IDLE, PlayerState.RUNNING]:
			_change_state(PlayerState.ATTACKING)
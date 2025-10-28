// PHP Extractor Tests - Direct port from Miller
//
// Test file: /Users/murphy/Source/miller/src/__tests__/parser/php-extractor.test.ts
// This file contains 5 comprehensive test cases covering all PHP language features
//
// Port Status: RED phase - All tests should fail initially until extractor is implemented

use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
use crate::extractors::php::PhpExtractor;
use tree_sitter::Parser;

// Helper function to initialize PHP parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("Error loading PHP grammar");
    parser
}

// Helper function to extract symbols from PHP code
fn extract_symbols(code: &str) -> Vec<Symbol> {
    use std::path::PathBuf;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PhpExtractor::new("php".to_string(), "test.php".to_string(), code.to_string(), &workspace_root);

    extractor.extract_symbols(&tree)
}

// Helper function to extract relationships
fn extract_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    use std::path::PathBuf;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PhpExtractor::new("php".to_string(), "test.php".to_string(), code.to_string(), &workspace_root);

    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[cfg(test)]
mod php_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_classes_interfaces_and_their_members() {
        let php_code = r#"<?php

namespace App\Models;

use App\Contracts\UserRepositoryInterface;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Facades\Hash;

interface Drawable
{
    public function draw(): void;
    public function getColor(): string;
    public function setColor(string $color): self;
}

interface Serializable
{
    public function serialize(): string;
    public function unserialize(string $data): void;
}

abstract class Shape implements Drawable
{
    protected string $color = 'black';
    protected static int $instanceCount = 0;

    public const DEFAULT_COLOR = 'white';
    private const MAX_SIZE = 1000;

    public function __construct(string $color = self::DEFAULT_COLOR)
    {
        $this->color = $color;
        self::$instanceCount++;
    }

    public function getColor(): string
    {
        return $this->color;
    }

    public function setColor(string $color): self
    {
        $this->color = $color;
        return $this;
    }

    abstract public function getArea(): float;

    public static function getInstanceCount(): int
    {
        return self::$instanceCount;
    }

    public function __toString(): string
    {
        return "Shape with color: {$this->color}";
    }

    public function __destruct()
    {
        self::$instanceCount--;
    }
}

class Circle extends Shape implements Serializable
{
    private float $radius;

    public function __construct(float $radius, string $color = parent::DEFAULT_COLOR)
    {
        parent::__construct($color);
        $this->radius = $radius;
    }

    public function draw(): void
    {
        echo "Drawing a circle with radius {$this->radius}\n";
    }

    public function getArea(): float
    {
        return pi() * $this->radius ** 2;
    }

    public function getRadius(): float
    {
        return $this->radius;
    }

    public function setRadius(float $radius): void
    {
        $this->radius = $radius;
    }

    public function serialize(): string
    {
        return json_encode([
            'radius' => $this->radius,
            'color' => $this->color
        ]);
    }

    public function unserialize(string $data): void
    {
        $decoded = json_decode($data, true);
        $this->radius = $decoded['radius'];
        $this->color = $decoded['color'];
    }
}

final class Rectangle extends Shape
{
    private float $width;
    private float $height;

    public function __construct(float $width, float $height, string $color = 'blue')
    {
        parent::__construct($color);
        $this->width = $width;
        $this->height = $height;
    }

    public function draw(): void
    {
        echo "Drawing a rectangle {$this->width}x{$this->height}\n";
    }

    public function getArea(): float
    {
        return $this->width * $this->height;
    }

    final public function getDimensions(): array
    {
        return [$this->width, $this->height];
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // Namespace
        let namespace = symbols.iter().find(|s| s.name == "App\\Models");
        assert!(namespace.is_some());
        assert_eq!(namespace.unwrap().kind, SymbolKind::Namespace);

        // Use statements
        let use_statement = symbols
            .iter()
            .find(|s| s.name == "App\\Contracts\\UserRepositoryInterface");
        assert!(use_statement.is_some());
        assert_eq!(use_statement.unwrap().kind, SymbolKind::Import);

        // Interface
        let drawable = symbols.iter().find(|s| s.name == "Drawable");
        assert!(drawable.is_some());
        assert_eq!(drawable.unwrap().kind, SymbolKind::Interface);

        // Interface methods
        let draw = symbols
            .iter()
            .find(|s| s.name == "draw" && s.parent_id.as_ref() == drawable.map(|d| &d.id));
        assert!(draw.is_some());
        assert!(draw
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public function draw(): void"));

        let get_color = symbols
            .iter()
            .find(|s| s.name == "getColor" && s.parent_id.as_ref() == drawable.map(|d| &d.id));
        assert!(get_color.is_some());
        assert!(get_color
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("getColor(): string"));

        let set_color = symbols
            .iter()
            .find(|s| s.name == "setColor" && s.parent_id.as_ref() == drawable.map(|d| &d.id));
        assert!(set_color.is_some());
        assert!(set_color
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("setColor(string $color): self"));

        // Abstract class
        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert!(shape
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("abstract class Shape implements Drawable"));

        // Properties with visibility and types
        let color = symbols.iter().find(|s| s.name == "color");
        assert!(color.is_some());
        assert_eq!(color.unwrap().kind, SymbolKind::Property);
        assert_eq!(
            color.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Protected
        );
        assert!(color
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("protected string $color = 'black'"));

        let instance_count = symbols.iter().find(|s| s.name == "instanceCount");
        assert!(instance_count.is_some());
        assert!(instance_count
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("protected static int $instanceCount = 0"));

        // Constants
        let default_color = symbols.iter().find(|s| s.name == "DEFAULT_COLOR");
        assert!(default_color.is_some());
        assert_eq!(default_color.unwrap().kind, SymbolKind::Constant);
        assert_eq!(
            default_color.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );
        assert!(default_color
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public const DEFAULT_COLOR = 'white'"));

        let max_size = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size.is_some());
        assert_eq!(
            max_size.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private
        );

        // Constructor with parameters and default values
        let constructor = symbols
            .iter()
            .find(|s| s.name == "__construct" && s.parent_id.as_ref() == shape.map(|sh| &sh.id));
        assert!(constructor.is_some());
        assert_eq!(constructor.unwrap().kind, SymbolKind::Constructor);
        assert!(constructor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("__construct(string $color = self::DEFAULT_COLOR)"));

        // Abstract method
        let get_area = symbols
            .iter()
            .find(|s| s.name == "getArea" && s.parent_id.as_ref() == shape.map(|sh| &sh.id));
        assert!(get_area.is_some());
        assert!(get_area
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("abstract public function getArea(): float"));

        // Static method
        let get_instance_count = symbols.iter().find(|s| s.name == "getInstanceCount");
        assert!(get_instance_count.is_some());
        assert!(get_instance_count
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public static function getInstanceCount(): int"));

        // Magic methods
        let to_string = symbols.iter().find(|s| s.name == "__toString");
        assert!(to_string.is_some());
        assert!(to_string
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public function __toString(): string"));

        let destructor = symbols.iter().find(|s| s.name == "__destruct");
        assert!(destructor.is_some());
        assert_eq!(destructor.unwrap().kind, SymbolKind::Destructor);

        // Concrete class with multiple interfaces
        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("class Circle extends Shape implements Serializable"));

        // Method with void return type
        let circle_draw = symbols
            .iter()
            .find(|s| s.name == "draw" && s.parent_id.as_ref() == circle.map(|c| &c.id));
        assert!(circle_draw.is_some());
        assert!(circle_draw
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public function draw(): void"));

        // Final class
        let rectangle = symbols.iter().find(|s| s.name == "Rectangle");
        assert!(rectangle.is_some());
        assert!(rectangle
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("final class Rectangle extends Shape"));

        // Final method
        let get_dimensions = symbols.iter().find(|s| s.name == "getDimensions");
        assert!(get_dimensions.is_some());
        assert!(get_dimensions
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("final public function getDimensions(): array"));
    }

    #[test]
    fn test_extract_traits_enums_and_modern_php_features() {
        let php_code = r#"<?php

namespace App\Enums;

use BackedEnum;
use JsonSerializable;

enum Status: string implements JsonSerializable
{
    case PENDING = 'pending';
    case APPROVED = 'approved';
    case REJECTED = 'rejected';

    public function getLabel(): string
    {
        return match($this) {
            self::PENDING => 'Pending Review',
            self::APPROVED => 'Approved',
            self::REJECTED => 'Rejected',
        };
    }

    public function jsonSerialize(): mixed
    {
        return $this->value;
    }
}

enum Priority: int
{
    case LOW = 1;
    case MEDIUM = 2;
    case HIGH = 3;
    case CRITICAL = 4;

    public function getColor(): string
    {
        return match($this) {
            self::LOW => 'green',
            self::MEDIUM => 'yellow',
            self::HIGH => 'orange',
            self::CRITICAL => 'red',
        };
    }
}

trait Timestampable
{
    protected ?\DateTime $createdAt = null;
    protected ?\DateTime $updatedAt = null;

    public function touch(): void
    {
        $this->updatedAt = new \DateTime();
    }

    public function getCreatedAt(): ?\DateTime
    {
        return $this->createdAt;
    }

    public function setCreatedAt(\DateTime $createdAt): self
    {
        $this->createdAt = $createdAt;
        return $this;
    }
}

trait Cacheable
{
    private static array $cache = [];

    public function getCacheKey(): string
    {
        return static::class . ':' . $this->getId();
    }

    public function cache(): void
    {
        self::$cache[$this->getCacheKey()] = $this;
    }

    public static function getFromCache(string $key): ?static
    {
        return self::$cache[$key] ?? null;
    }

    abstract public function getId(): int|string;
}

#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_METHOD)]
class ApiResource
{
    public function __construct(
        public readonly string $version = 'v1',
        public readonly bool $deprecated = false,
        public readonly array $scopes = []
    ) {}
}

#[Attribute(Attribute::TARGET_PROPERTY)]
class Validate
{
    public function __construct(
        public readonly array $rules = [],
        public readonly ?string $message = null
    ) {}
}

#[ApiResource(version: 'v2', scopes: ['read', 'write'])]
class User
{
    use Timestampable;
    use Cacheable;

    private const DEFAULT_ROLE = 'user';

    #[Validate(rules: ['required', 'string', 'max:255'])]
    private string $name;

    #[Validate(rules: ['required', 'email', 'unique:users'])]
    private string $email;

    private ?string $password = null;
    private Status $status = Status::PENDING;
    private Priority $priority = Priority::LOW;

    public function __construct(
        string $name,
        string $email,
        ?string $password = null,
        private readonly int $id = 0
    ) {
        $this->name = $name;
        $this->email = $email;
        $this->password = $password ? password_hash($password, PASSWORD_DEFAULT) : null;
        $this->createdAt = new \DateTime();
        $this->updatedAt = new \DateTime();
    }

    public function getId(): int
    {
        return $this->id;
    }

    public function getName(): string
    {
        return $this->name;
    }

    public function setName(string $name): void
    {
        $this->name = $name;
        $this->touch();
    }

    #[ApiResource(deprecated: true)]
    public function getEmail(): string
    {
        return $this->email;
    }

    public function updateStatus(Status $status): void
    {
        $this->status = $status;
        $this->touch();
    }

    public function getStatus(): Status
    {
        return $this->status;
    }

    public function hasHighPriority(): bool
    {
        return $this->priority === Priority::HIGH || $this->priority === Priority::CRITICAL;
    }
}

readonly class Configuration
{
    public function __construct(
        public string $database_url,
        public string $api_key,
        public bool $debug_mode = false,
        public array $features = []
    ) {}

    public function isFeatureEnabled(string $feature): bool
    {
        return in_array($feature, $this->features);
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // Backed enum
        let status = symbols.iter().find(|s| s.name == "Status");
        assert!(status.is_some());
        assert_eq!(status.unwrap().kind, SymbolKind::Enum);
        assert!(status
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("enum Status: string implements JsonSerializable"));

        // Enum cases
        let pending = symbols.iter().find(|s| s.name == "PENDING");
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().kind, SymbolKind::EnumMember);
        assert!(pending
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("case PENDING = 'pending'"));

        // Enum method
        let get_label = symbols
            .iter()
            .find(|s| s.name == "getLabel" && s.parent_id.as_ref() == status.map(|st| &st.id));
        assert!(get_label.is_some());
        assert!(get_label
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public function getLabel(): string"));

        // Int enum
        let priority = symbols.iter().find(|s| s.name == "Priority");
        assert!(priority.is_some());
        assert!(priority
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("enum Priority: int"));

        let low = symbols.iter().find(|s| s.name == "LOW");
        assert!(low.is_some());
        assert!(low
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("case LOW = 1"));

        // Trait
        let timestampable = symbols.iter().find(|s| s.name == "Timestampable");
        assert!(timestampable.is_some());
        assert_eq!(timestampable.unwrap().kind, SymbolKind::Trait);

        // Trait properties with nullable types
        let created_at = symbols.iter().find(|s| s.name == "createdAt");
        assert!(created_at.is_some());
        assert!(created_at
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("protected ?\\DateTime $createdAt = null"));

        // Trait method
        let touch = symbols
            .iter()
            .find(|s| s.name == "touch" && s.parent_id.as_ref() == timestampable.map(|t| &t.id));
        assert!(touch.is_some());

        // Trait with static property
        let cacheable = symbols.iter().find(|s| s.name == "Cacheable");
        assert!(cacheable.is_some());

        let cache = symbols.iter().find(|s| {
            s.name == "cache"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("private static array")
        });
        assert!(cache.is_some());
        assert!(cache
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("private static array $cache = []"));

        // Abstract method in trait
        let get_id = symbols
            .iter()
            .find(|s| s.name == "getId" && s.signature.as_ref().unwrap().contains("abstract"));
        assert!(get_id.is_some());
        assert!(get_id
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("abstract public function getId(): int|string"));

        // Attribute class
        let api_resource = symbols.iter().find(|s| s.name == "ApiResource");
        assert!(api_resource.is_some());
        assert!(api_resource
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_METHOD)]"));

        // Constructor property promotion
        let api_constructor = symbols.iter().find(|s| {
            s.name == "__construct" && s.parent_id.as_ref() == api_resource.map(|ar| &ar.id)
        });
        assert!(api_constructor.is_some());
        assert!(api_constructor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public readonly string $version = 'v1'"));

        // Class with attributes
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[ApiResource(version: 'v2', scopes: ['read', 'write'])]"));
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use Timestampable"));
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use Cacheable"));

        // Property with attribute
        let name = symbols
            .iter()
            .find(|s| s.name == "name" && s.parent_id.as_ref() == user.map(|u| &u.id));
        assert!(name.is_some());
        assert!(name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Validate(rules: ['required', 'string', 'max:255'])]"));

        // Union type
        let trait_get_id = symbols
            .iter()
            .find(|s| s.name == "getId" && s.parent_id.as_ref() == user.map(|u| &u.id));
        assert!(trait_get_id.is_some());
        assert!(trait_get_id
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("getId(): int"));

        // Method with attribute
        let get_email = symbols.iter().find(|s| s.name == "getEmail");
        assert!(get_email.is_some());
        assert!(get_email
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[ApiResource(deprecated: true)]"));

        // Readonly class
        let configuration = symbols.iter().find(|s| s.name == "Configuration");
        assert!(configuration.is_some());
        assert!(configuration
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("readonly class Configuration"));

        // Constructor with property promotion
        let config_constructor = symbols.iter().find(|s| {
            s.name == "__construct" && s.parent_id.as_ref() == configuration.map(|c| &c.id)
        });
        assert!(config_constructor.is_some());
        assert!(config_constructor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public string $database_url"));
        assert!(config_constructor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public bool $debug_mode = false"));
    }

    #[test]
    fn test_extract_global_functions_closures_and_anonymous_functions() {
        let php_code = r#"<?php

function calculateSum(int $a, int $b): int
{
    return $a + $b;
}

function processData(array $data, callable $callback = null): array
{
    if ($callback === null) {
        $callback = fn($item) => $item * 2;
    }

    return array_map($callback, $data);
}

function createMultiplier(int $factor): \Closure
{
    return function(int $number) use ($factor): int {
        return $number * $factor;
    };
}

function &getReference(array &$array, string $key): mixed
{
    return $array[$key];
}

function formatString(string $template, string ...$args): string
{
    return sprintf($template, ...$args);
}

// Type declarations and union types
function handleValue(int|string|null $value): string
{
    return match(true) {
        is_int($value) => "Integer: $value",
        is_string($value) => "String: $value",
        is_null($value) => "Null value",
    };
}

function processUser(
    string $name,
    int $age,
    ?string $email = null,
    array $options = []
): array {
    return compact('name', 'age', 'email', 'options');
}

// Arrow functions
$numbers = [1, 2, 3, 4, 5];
$doubled = array_map(fn($n) => $n * 2, $numbers);
$filtered = array_filter($numbers, fn($n) => $n > 2);

// Regular closures
$multiplier = function(int $x, int $y): int {
    return $x * $y;
};

$processor = function(array $items) use ($multiplier): array {
    return array_map(fn($item) => $multiplier($item, 2), $items);
};

// Closure with reference capture
$counter = 0;
$incrementer = function() use (&$counter): int {
    return ++$counter;
};

// First-class callable syntax (PHP 8.1+)
$strlen = strlen(...);
$array_map = array_map(...);

class MathOperations
{
    public static function add(int $a, int $b): int
    {
        return $a + $b;
    }

    public function multiply(int $a, int $b): int
    {
        return $a * $b;
    }
}

// Method references
$add = MathOperations::add(...);
$instance = new MathOperations();
$multiply = $instance->multiply(...);

// Anonymous classes
$logger = new class implements \Psr\Log\LoggerInterface {
    public function log($level, $message, array $context = []): void
    {
        echo "[$level] $message\n";
    }

    public function info($message, array $context = []): void
    {
        $this->log('info', $message, $context);
    }

    // Implement other PSR-3 methods...
    public function emergency($message, array $context = []): void {}
    public function alert($message, array $context = []): void {}
    public function critical($message, array $context = []): void {}
    public function error($message, array $context = []): void {}
    public function warning($message, array $context = []): void {}
    public function notice($message, array $context = []): void {}
    public function debug($message, array $context = []): void {}
};
"#;

        let symbols = extract_symbols(php_code);

        // Global function with type declarations
        let calculate_sum = symbols.iter().find(|s| s.name == "calculateSum");
        assert!(calculate_sum.is_some());
        assert_eq!(calculate_sum.unwrap().kind, SymbolKind::Function);
        assert!(calculate_sum
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function calculateSum(int $a, int $b): int"));

        // Function with callable parameter
        let process_data = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data.is_some());
        assert!(process_data
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("callable $callback = null"));

        // Function returning closure
        let create_multiplier = symbols.iter().find(|s| s.name == "createMultiplier");
        assert!(create_multiplier.is_some());
        assert!(create_multiplier
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("createMultiplier(int $factor): \\Closure"));

        // Function returning reference
        let get_reference = symbols.iter().find(|s| s.name == "getReference");
        assert!(get_reference.is_some());
        assert!(get_reference
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function &getReference(array &$array, string $key): mixed"));

        // Function with variadic parameters
        let format_string = symbols.iter().find(|s| s.name == "formatString");
        assert!(format_string.is_some());
        assert!(format_string
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("string ...$args"));

        // Function with union types
        let handle_value = symbols.iter().find(|s| s.name == "handleValue");
        assert!(handle_value.is_some());
        assert!(handle_value
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("int|string|null $value"));

        // Function with complex parameters
        let process_user = symbols.iter().find(|s| s.name == "processUser");
        assert!(process_user.is_some());
        assert!(process_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("?string $email = null"));
        assert!(process_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("array $options = []"));

        // Variable assignments with closures
        let doubled = symbols.iter().find(|s| s.name == "doubled");
        assert!(doubled.is_some());
        assert!(doubled
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("array_map(fn($n) => $n * 2, $numbers)"));

        let multiplier = symbols.iter().find(|s| s.name == "multiplier");
        assert!(multiplier.is_some());
        assert!(multiplier
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function(int $x, int $y): int"));

        let processor = symbols.iter().find(|s| s.name == "processor");
        assert!(processor.is_some());
        assert!(processor
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use ($multiplier)"));

        // Closure with reference capture
        let incrementer = symbols.iter().find(|s| s.name == "incrementer");
        assert!(incrementer.is_some());
        assert!(incrementer
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use (&$counter)"));

        // First-class callable syntax
        let strlen = symbols
            .iter()
            .find(|s| s.name == "strlen" && s.signature.as_ref().unwrap().contains("strlen(...)"));
        assert!(strlen.is_some());

        // Class for method references
        let math_operations = symbols.iter().find(|s| s.name == "MathOperations");
        assert!(math_operations.is_some());

        let add = symbols
            .iter()
            .find(|s| s.name == "add" && s.parent_id.as_ref() == math_operations.map(|mo| &mo.id));
        assert!(add.is_some());
        assert!(add
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public static function add(int $a, int $b): int"));

        // Method references
        let add_ref = symbols.iter().find(|s| {
            s.name == "add"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("MathOperations::add(...)")
        });
        assert!(add_ref.is_some());

        // Anonymous class
        let logger = symbols.iter().find(|s| s.name == "logger");
        assert!(logger.is_some());
        assert!(logger
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("new class implements \\Psr\\Log\\LoggerInterface"));
    }

    #[test]
    fn test_infer_types_from_php_type_declarations_and_doc_comments() {
        let php_code = r#"<?php

class UserService
{
    public function findById(int $id): ?User
    {
        return User::find($id);
    }

    public function getUsers(): array
    {
        return User::all();
    }

    public function createUser(string $name, string $email): User
    {
        return new User($name, $email);
    }

    public function updateUser(User $user, array $data): bool
    {
        return $user->update($data);
    }

    /**
     * @return User[]
     */
    public function getActiveUsers(): array
    {
        return User::where('active', true)->get();
    }

    /**
     * @param array<string, mixed> $filters
     * @return Collection<User>
     */
    public function searchUsers(array $filters): \Illuminate\Support\Collection
    {
        return User::filter($filters);
    }

    private string $apiKey = 'default-key';
    private ?\DateTime $lastSync = null;
    private array $cache = [];
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        // Function return types
        let find_by_id = symbols.iter().find(|s| s.name == "findById").unwrap();
        assert_eq!(types.get(&find_by_id.id), Some(&"?User".to_string()));

        let get_users = symbols.iter().find(|s| s.name == "getUsers").unwrap();
        assert_eq!(types.get(&get_users.id), Some(&"array".to_string()));

        let create_user = symbols.iter().find(|s| s.name == "createUser").unwrap();
        assert_eq!(types.get(&create_user.id), Some(&"User".to_string()));

        let update_user = symbols.iter().find(|s| s.name == "updateUser").unwrap();
        assert_eq!(types.get(&update_user.id), Some(&"bool".to_string()));

        // Property types
        let api_key = symbols.iter().find(|s| s.name == "apiKey").unwrap();
        assert_eq!(types.get(&api_key.id), Some(&"string".to_string()));

        let last_sync = symbols.iter().find(|s| s.name == "lastSync").unwrap();
        assert_eq!(types.get(&last_sync.id), Some(&"?\\DateTime".to_string()));

        let cache = symbols.iter().find(|s| s.name == "cache").unwrap();
        assert_eq!(types.get(&cache.id), Some(&"array".to_string()));
    }

    #[test]
    fn test_extract_inheritance_and_interface_implementation_relationships() {
        let php_code = r#"<?php

interface Drawable
{
    public function draw(): void;
}

interface Colorable
{
    public function getColor(): string;
    public function setColor(string $color): void;
}

abstract class Shape implements Drawable
{
    protected string $color;

    public function __construct(string $color)
    {
        $this->color = $color;
    }

    public function getColor(): string
    {
        return $this->color;
    }

    abstract public function getArea(): float;
}

class Circle extends Shape implements Colorable
{
    private float $radius;

    public function __construct(float $radius, string $color)
    {
        parent::__construct($color);
        $this->radius = $radius;
    }

    public function draw(): void
    {
        echo "Drawing circle";
    }

    public function setColor(string $color): void
    {
        $this->color = $color;
    }

    public function getArea(): float
    {
        return pi() * $this->radius ** 2;
    }
}

class Rectangle extends Shape implements Colorable
{
    private float $width;
    private float $height;

    public function __construct(float $width, float $height, string $color)
    {
        parent::__construct($color);
        $this->width = $width;
        $this->height = $height;
    }

    public function draw(): void
    {
        echo "Drawing rectangle";
    }

    public function setColor(string $color): void
    {
        $this->color = $color;
    }

    public function getArea(): float
    {
        return $this->width * $this->height;
    }
}
"#;

        let (symbols, relationships) = extract_relationships(php_code);

        // Should find inheritance and interface implementation relationships
        assert!(relationships.len() >= 4);

        // Shape implements Drawable
        let shape_drawable = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Implements
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Shape".to_string())
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Drawable".to_string())
        });
        assert!(shape_drawable.is_some());

        // Circle extends Shape
        let circle_shape = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Circle".to_string())
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Shape".to_string())
        });
        assert!(circle_shape.is_some());

        // Circle implements Colorable
        let circle_colorable = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Implements
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Circle".to_string())
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Colorable".to_string())
        });
        assert!(circle_colorable.is_some());

        // Rectangle extends Shape
        let rectangle_shape = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Rectangle".to_string())
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Shape".to_string())
        });
        assert!(rectangle_shape.is_some());

        // Rectangle implements Colorable
        let rectangle_colorable = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Implements
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Rectangle".to_string())
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| &s.name)
                    == Some(&"Colorable".to_string())
        });
        assert!(rectangle_colorable.is_some());
    }
}

// ========================================================================
// PHP Identifier Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (function_call_expression)
// - Member access (member_access_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#[cfg(test)]
mod identifier_extraction {
    use super::*;
    use crate::extractors::base::IdentifierKind;

    #[test]
    fn test_extract_function_calls() {
        let php_code = r#"<?php

class Calculator {
    public function add(int $a, int $b): int {
        return $a + $b;
    }

    public function calculate(): int {
        $result = $this->add(5, 3);      // Function call to add
        echo print_r($result, true);     // Function call to print_r
        return $result;
    }
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' function call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let print_r_call = identifiers.iter().find(|id| id.name == "print_r");
        assert!(
            print_r_call.is_some(),
            "Should extract 'print_r' function call identifier"
        );
        let print_r_call = print_r_call.unwrap();
        assert_eq!(print_r_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate method)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
        );

        // Find the calculate method symbol
        let calculate_method = symbols.iter().find(|s| s.name == "calculate").unwrap();

        // Verify the add call is contained within calculate method
        assert_eq!(
            add_call.containing_symbol_id.as_ref(),
            Some(&calculate_method.id),
            "add call should be contained within calculate method"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let php_code = r#"<?php

class User {
    public string $name;
    public string $email;

    public function printInfo(): void {
        echo $this->name;         // Member access: $this->name
        $email = $this->email;    // Member access: $this->email
    }
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' member access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' member access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let php_code = r#"<?php

class Service {
    public function process(): void {
        $this->helper();              // Call to helper in same file
    }

    private function helper(): void {
        // Helper method
    }
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the helper call
        let helper_call = identifiers.iter().find(|id| id.name == "helper");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the process method)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "helper call should have containing symbol from same file"
        );

        // Verify the containing symbol is the process method
        let process_method = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&process_method.id),
            "helper call should be contained within process method"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let php_code = r#"<?php

class DataService {
    public function execute(): void {
        $result = $user->account->balance;      // Chained member access
        $name = $customer->profile->name;       // Chained member access
    }
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );

        let name_access = identifiers
            .iter()
            .find(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            name_access.is_some(),
            "Should extract 'name' from chained member access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let php_code = r#"<?php

class Test {
    public function run(): void {
        $this->process();
        $this->process();  // Same call twice
    }

    private function process(): void {
    }
}
"#;

        use std::path::PathBuf;
        let mut parser = init_parser();
        let tree = parser.parse(php_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = PhpExtractor::new(
            "php".to_string(),
            "test.php".to_string(),
            php_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let process_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            process_calls.len(),
            2,
            "Should extract both process calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            process_calls[0].start_line, process_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }

    #[test]
    fn test_php_malformed_syntax() {
        let php_code = r#"<?php
class Test {
    public function method() {
        // Missing closing brace
        if (true) {
            echo "test";
        // Missing closing parenthesis and brace
        function broken( {
            return "broken";
        }
    }
"#;

        let symbols = extract_symbols(php_code);

        // Should handle malformed PHP gracefully
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_php_unicode_and_special_chars() {
        let php_code = r#"<?php
class Café {
    public function método() {
        $variable = "tëst";
        $emoji = "🚀";
        return $variable . $emoji;
    }
}

function función_ñ() {
    return "español";
}
"#;

        let symbols = extract_symbols(php_code);

        // Should handle Unicode characters in identifiers
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_php_heredoc_and_nowdoc() {
        let php_code = r#"<?php
class Template {
    public function getHeredoc(): string {
        return <<<HTML
<div class="content">
    <h1>Title</h1>
    <p>Content with "quotes" and 'apostrophes'</p>
</div>
HTML;
    }

    public function getNowdoc(): string {
        return <<<'SQL'
SELECT * FROM users
WHERE active = 1
AND name LIKE '%test%'
SQL;
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // Should handle heredoc and nowdoc syntax
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_php_dynamic_features() {
        let php_code = r#"<?php
class Dynamic {
    public function __call($method, $args) {
        return "Called: $method";
    }

    public static function __callStatic($method, $args) {
        return "Static called: $method";
    }

    public function __get($property) {
        return "Getting: $property";
    }

    public function __set($property, $value) {
        $this->$property = $value;
    }
}

function variable_function() {
    return "variable function result";
}

$func = 'variable_function';
$result = $func();
"#;

        let symbols = extract_symbols(php_code);

        // Should handle dynamic PHP features
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_namespace_usage_and_aliasing() {
        let php_code = r#"<?php

namespace App\Services;

use App\Models\User;
use App\Contracts\UserRepositoryInterface as UserRepo;
use Illuminate\Support\Collection;
use Illuminate\Database\Eloquent\Model as EloquentModel;
use function App\Helpers\formatDate;
use const App\Config\DEFAULT_TIMEOUT;

class UserService
{
    private UserRepo $repository;
    private EloquentModel $model;

    public function __construct(UserRepo $repository)
    {
        $this->repository = $repository;
        $this->model = new EloquentModel();
    }

    public function findUser(int $id): ?User
    {
        return $this->repository->find($id);
    }

    public function getAllUsers(): Collection
    {
        return $this->repository->all();
    }

    public function formatUserDate(User $user): string
    {
        return formatDate($user->created_at);
    }

    public function getTimeout(): int
    {
        return DEFAULT_TIMEOUT;
    }
}

namespace App\Controllers;

use App\Services\UserService;
use App\Models\User;

class UserController
{
    private UserService $service;

    public function __construct(UserService $service)
    {
        $this->service = $service;
    }

    public function show(int $id): ?User
    {
        return $this->service->findUser($id);
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // First namespace
        let app_services = symbols.iter().find(|s| s.name == "App\\Services");
        assert!(app_services.is_some());
        assert_eq!(app_services.unwrap().kind, SymbolKind::Namespace);

        // Second namespace
        let app_controllers = symbols.iter().find(|s| s.name == "App\\Controllers");
        assert!(app_controllers.is_some());
        assert_eq!(app_controllers.unwrap().kind, SymbolKind::Namespace);

        // Use statements with aliases
        let user_repo = symbols
            .iter()
            .find(|s| s.name == "App\\Contracts\\UserRepositoryInterface");
        assert!(user_repo.is_some());
        assert_eq!(user_repo.unwrap().kind, SymbolKind::Import);

        let eloquent_model = symbols
            .iter()
            .find(|s| s.name == "Illuminate\\Database\\Eloquent\\Model");
        assert!(eloquent_model.is_some());
        assert_eq!(eloquent_model.unwrap().kind, SymbolKind::Import);

        // Function import
        let format_date = symbols
            .iter()
            .find(|s| s.name == "App\\Helpers\\formatDate");
        assert!(format_date.is_some());
        assert_eq!(format_date.unwrap().kind, SymbolKind::Import);

        // Const import
        let default_timeout = symbols
            .iter()
            .find(|s| s.name == "App\\Config\\DEFAULT_TIMEOUT");
        assert!(default_timeout.is_some());
        assert_eq!(default_timeout.unwrap().kind, SymbolKind::Import);

        // Classes in namespaces
        let user_service = symbols.iter().find(|s| s.name == "UserService");
        assert!(user_service.is_some());
        assert_eq!(user_service.unwrap().kind, SymbolKind::Class);

        let user_controller = symbols.iter().find(|s| s.name == "UserController");
        assert!(user_controller.is_some());
        assert_eq!(user_controller.unwrap().kind, SymbolKind::Class);

        // Verify namespace-qualified types in signatures
        let find_user = symbols.iter().find(|s| {
            s.name == "findUser" && s.parent_id.as_ref() == user_service.map(|us| &us.id)
        });
        assert!(find_user.is_some());
        assert!(find_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("findUser(int $id): ?User"));
    }

    #[test]
    fn test_extract_trait_usage_and_conflicts() {
        let php_code = r#"<?php

trait Loggable
{
    protected array $logs = [];

    public function log(string $message): void
    {
        $this->logs[] = $message;
    }

    public function getLogs(): array
    {
        return $this->logs;
    }
}

trait Timestampable
{
    protected ?\DateTime $createdAt = null;
    protected ?\DateTime $updatedAt = null;

    public function touch(): void
    {
        $this->updatedAt = new \DateTime();
    }

    public function getCreatedAt(): ?\DateTime
    {
        return $this->createdAt;
    }

    public function setCreatedAt(\DateTime $createdAt): self
    {
        $this->createdAt = $createdAt;
        return $this;
    }
}

trait Cacheable
{
    private static array $cache = [];
    private string $cacheKey;

    public function getCacheKey(): string
    {
        return $this->cacheKey ??= static::class . ':' . $this->getId();
    }

    public function cache(): void
    {
        self::$cache[$this->getCacheKey()] = $this;
    }

    public static function getFromCache(string $key): ?static
    {
        return self::$cache[$key] ?? null;
    }

    abstract public function getId(): int|string;
}

class User
{
    use Loggable, Timestampable, Cacheable {
        Loggable::log insteadof Timestampable;
        Timestampable::touch as protected touchTimestamp;
        Cacheable::getCacheKey as protected;
    }

    private int $id;
    private string $name;
    private string $email;

    public function __construct(int $id, string $name, string $email)
    {
        $this->id = $id;
        $this->name = $name;
        $this->email = $email;
        $this->createdAt = new \DateTime();
    }

    public function getId(): int
    {
        return $this->id;
    }

    public function getName(): string
    {
        return $this->name;
    }

    public function save(): void
    {
        $this->log("Saving user: {$this->name}");
        $this->touchTimestamp();
        $this->cache();
    }

    public function load(): ?self
    {
        return self::getFromCache("User:{$this->id}");
    }
}

class Product
{
    use Timestampable;

    private int $id;
    private string $name;
    private float $price;

    public function __construct(int $id, string $name, float $price)
    {
        $this->id = $id;
        $this->name = $name;
        $this->price = $price;
        $this->createdAt = new \DateTime();
    }

    public function getId(): int
    {
        return $this->id;
    }

    public function updatePrice(float $newPrice): void
    {
        $this->price = $newPrice;
        $this->touch();
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // Traits
        let loggable = symbols.iter().find(|s| s.name == "Loggable");
        assert!(loggable.is_some());
        assert_eq!(loggable.unwrap().kind, SymbolKind::Trait);

        let timestampable = symbols.iter().find(|s| s.name == "Timestampable");
        assert!(timestampable.is_some());
        assert_eq!(timestampable.unwrap().kind, SymbolKind::Trait);

        let cacheable = symbols.iter().find(|s| s.name == "Cacheable");
        assert!(cacheable.is_some());
        assert_eq!(cacheable.unwrap().kind, SymbolKind::Trait);

        // Class using multiple traits with conflict resolution
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use Loggable, Timestampable, Cacheable"));

        // Verify trait methods are defined in traits themselves
        let trait_log_method = symbols
            .iter()
            .find(|s| s.name == "log" && s.parent_id.as_ref() == loggable.map(|l| &l.id));
        assert!(trait_log_method.is_some());

        let trait_touch_method = symbols
            .iter()
            .find(|s| s.name == "touch" && s.parent_id.as_ref() == timestampable.map(|t| &t.id));
        assert!(trait_touch_method.is_some());

        // Abstract method implementation in User class (required by Cacheable trait)
        let get_id = symbols
            .iter()
            .find(|s| s.name == "getId" && s.parent_id.as_ref() == user.map(|u| &u.id));
        assert!(get_id.is_some());
        assert!(get_id
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("getId(): int"));

        // Class using single trait
        let product = symbols.iter().find(|s| s.name == "Product");
        assert!(product.is_some());
        assert!(product
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("use Timestampable"));
    }

    #[test]
    fn test_extract_php8_attributes_and_annotations() {
        let php_code = r#"<?php

#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_METHOD)]
class Route
{
    public function __construct(
        public string $path,
        public array $methods = ['GET'],
        public ?string $name = null
    ) {}
}

#[Attribute(Attribute::TARGET_PROPERTY)]
class Validate
{
    public function __construct(
        public array $rules = [],
        public ?string $message = null,
        public bool $required = true
    ) {}
}

#[Attribute(Attribute::TARGET_METHOD)]
class Deprecated
{
    public function __construct(
        public string $message = 'This method is deprecated',
        public ?string $since = null
    ) {}
}

#[Attribute(Attribute::TARGET_CLASS)]
class ApiResource
{
    public function __construct(
        public string $version = 'v1',
        public bool $deprecated = false,
        public array $scopes = []
    ) {}
}

#[ApiResource(version: 'v2', scopes: ['read', 'write', 'delete'])]
class UserController
{
    #[Validate(['required', 'integer', 'min:1'])]
    private int $userId;

    #[Validate(['required', 'string', 'max:255'])]
    private string $name;

    #[Validate(['required', 'email:rfc,dns'])]
    private string $email;

    public function __construct()
    {
        // Constructor
    }

    #[Route('/users', ['GET'])]
    public function index(): array
    {
        return [];
    }

    #[Route('/users', ['POST'])]
    #[Validate(['name' => 'required|string', 'email' => 'required|email'])]
    public function store(array $data): User
    {
        return new User($data['name'], $data['email']);
    }

    #[Route('/users/{id}', ['GET'])]
    public function show(#[Validate(['integer', 'min:1'])] int $id): ?User
    {
        return null;
    }

    #[Route('/users/{id}', ['PUT'])]
    #[Deprecated('Use update() instead', since: '2.0')]
    public function edit(int $id, array $data): User
    {
        return new User('name', 'email');
    }

    #[Route('/users/{id}', ['PATCH'])]
    public function update(int $id, array $data): User
    {
        return new User('name', 'email');
    }

    #[Route('/users/{id}', ['DELETE'])]
    public function destroy(int $id): bool
    {
        return true;
    }
}

class User
{
    public function __construct(
        public string $name,
        public string $email
    ) {}
}

#[Attribute(Attribute::TARGET_ALL)]
class Metadata
{
    public function __construct(public array $data = []) {}
}

#[Metadata(['author' => 'John Doe', 'version' => '1.0'])]
class Configuration
{
    #[Metadata(['description' => 'Database host'])]
    public string $host;

    #[Metadata(['description' => 'Database port', 'default' => 3306])]
    public int $port;

    public function __construct()
    {
        $this->host = 'localhost';
        $this->port = 3306;
    }
}
"#;

        let symbols = extract_symbols(php_code);

        // Attribute classes
        let route_attr = symbols.iter().find(|s| s.name == "Route");
        assert!(route_attr.is_some());
        assert!(route_attr
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_METHOD)]"));

        let validate_attr = symbols.iter().find(|s| s.name == "Validate");
        assert!(validate_attr.is_some());
        assert!(validate_attr
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_PROPERTY)]"));

        let deprecated_attr = symbols.iter().find(|s| s.name == "Deprecated");
        assert!(deprecated_attr.is_some());
        assert!(deprecated_attr
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_METHOD)]"));

        let api_resource_attr = symbols.iter().find(|s| s.name == "ApiResource");
        assert!(api_resource_attr.is_some());
        assert!(api_resource_attr
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_CLASS)]"));

        // Class with attributes
        let user_controller = symbols.iter().find(|s| s.name == "UserController");
        assert!(user_controller.is_some());
        assert!(user_controller
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[ApiResource(version: 'v2', scopes: ['read', 'write', 'delete'])]"));

        // Properties with attributes
        let user_id = symbols.iter().find(|s| s.name == "userId");
        assert!(user_id.is_some());
        assert!(user_id
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Validate(['required', 'integer', 'min:1'])]"));

        let name_prop = symbols.iter().find(|s| s.name == "name");
        assert!(name_prop.is_some());
        assert!(name_prop
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Validate(['required', 'string', 'max:255'])]"));

        // Methods with attributes
        let index_method = symbols.iter().find(|s| s.name == "index");
        assert!(index_method.is_some());
        assert!(index_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Route('/users', ['GET'])]"));

        let store_method = symbols.iter().find(|s| s.name == "store");
        assert!(store_method.is_some());
        assert!(store_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Route('/users', ['POST'])]"));
        assert!(store_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Validate(['name' => 'required|string', 'email' => 'required|email'])]"));

        let edit_method = symbols.iter().find(|s| s.name == "edit");
        assert!(edit_method.is_some());
        assert!(edit_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Deprecated('Use update() instead', since: '2.0')]"));

        // Parameter attributes (PHP 8)
        let show_method = symbols.iter().find(|s| s.name == "show");
        assert!(show_method.is_some());
        assert!(show_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Validate(['integer', 'min:1'])] int $id"));

        // Multiple attributes on same target
        let metadata_attr = symbols.iter().find(|s| s.name == "Metadata");
        assert!(metadata_attr.is_some());
        assert!(metadata_attr
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Attribute(Attribute::TARGET_ALL)]"));

        let configuration = symbols.iter().find(|s| s.name == "Configuration");
        assert!(configuration.is_some());
        assert!(configuration
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("#[Metadata(['author' => 'John Doe', 'version' => '1.0'])]"));
    }

    #[test]
    fn test_extract_phpdoc_from_class() {
        let php_code = r#"<?php
        /**
         * UserService manages user authentication and account operations.
         * Provides login, logout, and user management functionality.
         *
         * @author John Doe
         * @version 2.0
         * @since 1.0
         */
        class UserService {
            public function authenticate() {}
        }
        "#;

        let symbols = extract_symbols(php_code);
        let class_symbol = symbols.iter().find(|s| s.name == "UserService");

        assert!(class_symbol.is_some());
        let class_sym = class_symbol.unwrap();
        assert!(
            class_sym.doc_comment.is_some(),
            "UserService should have a doc comment"
        );

        let doc = class_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("manages user authentication"),
            "Doc should contain class description"
        );
        assert!(doc.contains("@author"), "Doc should contain @author tag");
        assert!(doc.contains("@version"), "Doc should contain @version tag");
    }

    #[test]
    fn test_extract_phpdoc_from_function() {
        let php_code = r#"<?php
        /**
         * Validates user credentials against the database.
         * Returns true if credentials are valid, false otherwise.
         *
         * @param string $username The username to validate
         * @param string $password The password to validate
         * @return bool True if valid, false otherwise
         * @throws InvalidArgumentException if username is empty
         */
        function validateCredentials($username, $password) {
            return true;
        }
        "#;

        let symbols = extract_symbols(php_code);
        let func = symbols.iter().find(|s| s.name == "validateCredentials");

        assert!(func.is_some());
        let func_sym = func.unwrap();
        assert!(
            func_sym.doc_comment.is_some(),
            "validateCredentials should have a doc comment"
        );

        let doc = func_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Validates user credentials"),
            "Doc should contain function description"
        );
        assert!(
            doc.contains("@param string $username"),
            "Doc should contain parameter documentation"
        );
        assert!(
            doc.contains("@return bool"),
            "Doc should contain return type documentation"
        );
        assert!(
            doc.contains("@throws InvalidArgumentException"),
            "Doc should contain exception documentation"
        );
    }

    #[test]
    fn test_extract_phpdoc_from_method() {
        let php_code = r#"<?php
        class UserRepository {
            /**
             * Find a user by ID.
             *
             * @param int $id The user ID
             * @return User|null The user if found, null otherwise
             */
            public function findById($id) {
                return null;
            }
        }
        "#;

        let symbols = extract_symbols(php_code);
        let method = symbols.iter().find(|s| s.name == "findById");

        assert!(method.is_some());
        let method_sym = method.unwrap();
        assert!(
            method_sym.doc_comment.is_some(),
            "findById method should have a doc comment"
        );

        let doc = method_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Find a user by ID"),
            "Doc should contain method description"
        );
        assert!(
            doc.contains("@param int $id"),
            "Doc should contain parameter documentation"
        );
        assert!(
            doc.contains("@return User|null"),
            "Doc should contain return type documentation"
        );
    }

    #[test]
    fn test_extract_phpdoc_from_property() {
        let php_code = r#"<?php
        class User {
            /**
             * The user's email address.
             * Must be a valid email format.
             *
             * @var string
             */
            private $email;
        }
        "#;

        let symbols = extract_symbols(php_code);
        let property = symbols.iter().find(|s| s.name == "email");

        assert!(property.is_some());
        let prop_sym = property.unwrap();
        assert!(
            prop_sym.doc_comment.is_some(),
            "email property should have a doc comment"
        );

        let doc = prop_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("user's email address"),
            "Doc should contain property description"
        );
        assert!(
            doc.contains("@var string"),
            "Doc should contain variable type documentation"
        );
    }

    #[test]
    fn test_extract_phpdoc_from_interface() {
        let php_code = r#"<?php
        /**
         * Defines the contract for serialization.
         * Implementing classes must provide serialization functionality.
         *
         * @since 1.5
         */
        interface Serializable {
            /**
             * Serialize the object to a string.
             *
             * @return string The serialized representation
             */
            public function serialize(): string;
        }
        "#;

        let symbols = extract_symbols(php_code);
        let interface = symbols.iter().find(|s| s.name == "Serializable");

        assert!(interface.is_some());
        let interface_sym = interface.unwrap();
        assert!(
            interface_sym.doc_comment.is_some(),
            "Serializable interface should have a doc comment"
        );

        let doc = interface_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("contract for serialization"),
            "Doc should contain interface description"
        );
        assert!(doc.contains("@since"), "Doc should contain @since tag");
    }

    #[test]
    fn test_extract_phpdoc_from_constant() {
        let php_code = r#"<?php
        class Config {
            /**
             * Maximum number of connection attempts.
             *
             * @var int
             */
            public const MAX_RETRIES = 3;
        }
        "#;

        let symbols = extract_symbols(php_code);
        let constant = symbols.iter().find(|s| s.name == "MAX_RETRIES");

        assert!(constant.is_some());
        let const_sym = constant.unwrap();
        assert!(
            const_sym.doc_comment.is_some(),
            "MAX_RETRIES constant should have a doc comment"
        );

        let doc = const_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Maximum number of connection attempts"),
            "Doc should contain constant description"
        );
    }

    #[test]
    fn test_phpdoc_extraction_mixed_symbols() {
        let php_code = r#"<?php
        /**
         * Handles payment processing
         */
        class PaymentProcessor {
            /**
             * Total amount processed.
             *
             * @var float
             */
            private $totalAmount = 0.0;

            /**
             * Process a payment transaction.
             *
             * @param float $amount The amount to process
             * @return bool Success status
             */
            public function processPayment($amount) {
                return true;
            }

            /**
             * Get the transaction history.
             *
             * @return array List of transactions
             */
            public function getHistory() {
                return [];
            }
        }
        "#;

        let symbols = extract_symbols(php_code);

        // Check class doc
        let class_sym = symbols
            .iter()
            .find(|s| s.name == "PaymentProcessor")
            .unwrap();
        assert!(class_sym.doc_comment.is_some());
        assert!(class_sym
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Handles payment processing"));

        // Check property doc
        let prop_sym = symbols.iter().find(|s| s.name == "totalAmount").unwrap();
        assert!(prop_sym.doc_comment.is_some());
        assert!(prop_sym
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Total amount processed"));

        // Check method docs
        let process_method = symbols.iter().find(|s| s.name == "processPayment").unwrap();
        assert!(process_method.doc_comment.is_some());
        assert!(process_method
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Process a payment"));

        let history_method = symbols.iter().find(|s| s.name == "getHistory").unwrap();
        assert!(history_method.doc_comment.is_some());
        assert!(history_method
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Get the transaction history"));
    }

    #[test]
    fn test_symbols_without_phpdoc_have_none() {
        let php_code = r#"<?php
        class SimpleClass {
            public $property;

            public function method() {}
        }
        "#;

        let symbols = extract_symbols(php_code);

        let class_sym = symbols.iter().find(|s| s.name == "SimpleClass").unwrap();
        assert!(
            class_sym.doc_comment.is_none(),
            "Class without doc comment should have None"
        );

        let prop_sym = symbols.iter().find(|s| s.name == "property").unwrap();
        assert!(
            prop_sym.doc_comment.is_none(),
            "Property without doc comment should have None"
        );

        let method_sym = symbols.iter().find(|s| s.name == "method").unwrap();
        assert!(
            method_sym.doc_comment.is_none(),
            "Method without doc comment should have None"
        );
    }
}

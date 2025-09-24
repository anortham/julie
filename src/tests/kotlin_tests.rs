// Kotlin Extractor Tests
//
// Direct port of Miller's Kotlin extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/kotlin-extractor.test.ts

use crate::extractors::base::{Symbol, SymbolKind, Visibility};
use crate::extractors::kotlin::KotlinExtractor;
use tree_sitter::Parser;

/// Initialize Kotlin parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_kotlin::language().into()).expect("Error loading Kotlin grammar");
    parser
}

#[cfg(test)]
mod kotlin_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_classes_and_data_classes() {
        let code = r#"
package com.example.models

import kotlinx.serialization.Serializable

class Vehicle(
    val brand: String,
    private var speed: Int = 0
) {
    fun accelerate() {
        speed += 10
    }

    fun getSpeed(): Int = speed

    companion object {
        const val MAX_SPEED = 200

        fun createDefault(): Vehicle {
            return Vehicle("Unknown")
        }
    }
}

data class Point(val x: Double, val y: Double) {
    fun distanceFromOrigin(): Double {
        return kotlin.math.sqrt(x * x + y * y)
    }
}

@Serializable
data class User(
    val id: Long,
    val name: String,
    val email: String?,
    val isActive: Boolean = true
)

abstract class Shape {
    abstract val area: Double
    abstract fun draw()

    open fun describe(): String {
        return "A shape with area $area"
    }
}

class Circle(private val radius: Double) : Shape() {
    override val area: Double
        get() = kotlin.math.PI * radius * radius

    override fun draw() {
        println("Drawing circle with radius $radius")
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Package declaration
        let package_symbol = symbols.iter().find(|s| s.name == "com.example.models");
        assert!(package_symbol.is_some());
        assert_eq!(package_symbol.unwrap().kind, SymbolKind::Namespace);

        // Import declaration
        let import_symbol = symbols.iter().find(|s| s.name == "kotlinx.serialization.Serializable");
        assert!(import_symbol.is_some());
        assert_eq!(import_symbol.unwrap().kind, SymbolKind::Import);

        // Regular class with primary constructor
        let vehicle = symbols.iter().find(|s| s.name == "Vehicle");
        assert!(vehicle.is_some());
        assert_eq!(vehicle.unwrap().kind, SymbolKind::Class);
        assert!(vehicle.unwrap().signature.as_ref().unwrap().contains("class Vehicle"));

        // Primary constructor parameters as properties
        let brand = symbols.iter().find(|s| s.name == "brand");
        assert!(brand.is_some());
        assert_eq!(brand.unwrap().kind, SymbolKind::Property);
        assert!(brand.unwrap().signature.as_ref().unwrap().contains("val brand: String"));

        let speed = symbols.iter().find(|s| s.name == "speed" && s.parent_id == Some(vehicle.unwrap().id.clone()));
        assert!(speed.is_some());
        assert_eq!(speed.unwrap().visibility.as_ref().unwrap(), &Visibility::Private);
        assert!(speed.unwrap().signature.as_ref().unwrap().contains("var speed: Int = 0"));

        // Methods
        let accelerate = symbols.iter().find(|s| s.name == "accelerate");
        assert!(accelerate.is_some());
        assert_eq!(accelerate.unwrap().kind, SymbolKind::Method);

        // Expression body function
        let get_speed = symbols.iter().find(|s| s.name == "getSpeed");
        assert!(get_speed.is_some());
        assert!(get_speed.unwrap().signature.as_ref().unwrap().contains("fun getSpeed(): Int = speed"));

        // Companion object
        let companion = symbols.iter().find(|s| s.name == "Companion");
        assert!(companion.is_some());
        assert!(companion.unwrap().signature.as_ref().unwrap().contains("companion object"));

        // Const val
        let max_speed = symbols.iter().find(|s| s.name == "MAX_SPEED");
        assert!(max_speed.is_some());
        assert_eq!(max_speed.unwrap().kind, SymbolKind::Constant);
        assert!(max_speed.unwrap().signature.as_ref().unwrap().contains("const val MAX_SPEED = 200"));

        // Data class
        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert!(point.unwrap().signature.as_ref().unwrap().contains("data class Point"));

        // Annotation on data class
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user.unwrap().signature.as_ref().unwrap().contains("@Serializable"));

        // Nullable type
        let email = symbols.iter().find(|s| s.name == "email");
        assert!(email.is_some());
        assert!(email.unwrap().signature.as_ref().unwrap().contains("String?"));

        // Default parameter
        let is_active = symbols.iter().find(|s| s.name == "isActive");
        assert!(is_active.is_some());
        assert!(is_active.unwrap().signature.as_ref().unwrap().contains("= true"));

        // Abstract class
        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert!(shape.unwrap().signature.as_ref().unwrap().contains("abstract class Shape"));

        // Abstract property
        let area = symbols.iter().find(|s| s.name == "area" && s.parent_id == Some(shape.unwrap().id.clone()));
        assert!(area.is_some());
        assert!(area.unwrap().signature.as_ref().unwrap().contains("abstract val area: Double"));

        // Override in subclass
        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle.unwrap().signature.contains("Circle(private val radius: Double) : Shape()"));

        let circle_area = symbols.iter().find(|s| s.name == "area" && s.parent_id == circle.unwrap().id);
        assert!(circle_area.is_some());
        assert!(circle_area.unwrap().signature.contains("override val area: Double"));
    }

    #[test]
    fn test_extract_objects_and_sealed_classes() {
        let code = r#"
object DatabaseConfig {
    const val URL = "jdbc:postgresql://localhost:5432/mydb"
    const val DRIVER = "org.postgresql.Driver"

    fun getConnection(): Connection {
        return DriverManager.getConnection(URL)
    }
}

object Utils : Serializable {
    fun formatDate(date: Date): String {
        return SimpleDateFormat("yyyy-MM-dd").format(date)
    }
}

sealed class Result<out T> {
    object Loading : Result<Nothing>()

    data class Success<T>(val data: T) : Result<T>()

    data class Error(
        val exception: Throwable,
        val message: String = exception.message ?: "Unknown error"
    ) : Result<Nothing>()
}

sealed interface Command {
    object Start : Command
    object Stop : Command
    data class Configure(val settings: Map<String, Any>) : Command
}

enum class Direction {
    NORTH, SOUTH, EAST, WEST;

    fun opposite(): Direction = when (this) {
        NORTH -> SOUTH
        SOUTH -> NORTH
        EAST -> WEST
        WEST -> EAST
    }
}

enum class Color(val rgb: Int) {
    RED(0xFF0000),
    GREEN(0x00FF00),
    BLUE(0x0000FF);

    companion object {
        fun fromRgb(rgb: Int): Color? {
            return values().find { it.rgb == rgb }
        }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Object declaration
        let database_config = symbols.iter().find(|s| s.name == "DatabaseConfig");
        assert!(database_config.is_some());
        assert!(database_config.unwrap().signature.contains("object DatabaseConfig"));

        // Object with inheritance
        let utils = symbols.iter().find(|s| s.name == "Utils");
        assert!(utils.is_some());
        assert!(utils.unwrap().signature.contains("object Utils : Serializable"));

        // Sealed class
        let result_symbol = symbols.iter().find(|s| s.name == "Result");
        assert!(result_symbol.is_some());
        assert!(result_symbol.unwrap().signature.contains("sealed class Result<out T>"));

        // Object inside sealed class
        let loading = symbols.iter().find(|s| s.name == "Loading");
        assert!(loading.is_some());
        assert!(loading.unwrap().signature.contains("object Loading : Result<Nothing>()"));

        // Data class extending sealed class
        let success = symbols.iter().find(|s| s.name == "Success");
        assert!(success.is_some());
        assert!(success.unwrap().signature.contains("data class Success<T>(val data: T) : Result<T>()"));

        // Sealed interface
        let command = symbols.iter().find(|s| s.name == "Command");
        assert!(command.is_some());
        assert!(command.unwrap().signature.contains("sealed interface Command"));

        // Simple enum
        let direction = symbols.iter().find(|s| s.name == "Direction");
        assert!(direction.is_some());
        assert_eq!(direction.unwrap().kind, SymbolKind::Enum);

        let north = symbols.iter().find(|s| s.name == "NORTH");
        assert!(north.is_some());
        assert_eq!(north.unwrap().kind, SymbolKind::EnumMember);

        // Enum with constructor
        let color = symbols.iter().find(|s| s.name == "Color");
        assert!(color.is_some());
        assert!(color.unwrap().signature.contains("enum class Color(val rgb: Int)"));

        let red = symbols.iter().find(|s| s.name == "RED");
        assert!(red.is_some());
        assert!(red.unwrap().signature.contains("RED(0xFF0000)"));

        // Companion object in enum
        let color_companion = symbols.iter().find(|s| s.name == "Companion" && s.parent_id == color.unwrap().id);
        assert!(color_companion.is_some());
    }

    #[test]
    fn test_extract_functions_and_extension_functions() {
        let code = r#"
fun greet(name: String): String {
    return "Hello, $name!"
}

fun calculateSum(vararg numbers: Int): Int = numbers.sum()

inline fun <reified T> Any?.isInstanceOf(): Boolean {
    return this is T
}

suspend fun fetchData(url: String): String {
    delay(1000)
    return "Data from $url"
}

fun String.isValidEmail(): Boolean {
    return this.contains("@") && this.contains(".")
}

fun List<String>.joinWithCommas(): String = this.joinToString(", ")

fun <T> Collection<T>.safeGet(index: Int): T? {
    return if (index in 0 until size) elementAtOrNull(index) else null
}

fun processData(
    data: List<String>,
    filter: (String) -> Boolean,
    transform: (String) -> String
): List<String> {
    return data.filter(filter).map(transform)
}

fun createProcessor(): (String) -> String {
    return { input -> input.uppercase() }
}

tailrec fun factorial(n: Long, accumulator: Long = 1): Long {
    return if (n <= 1) accumulator else factorial(n - 1, n * accumulator)
}

infix fun String.shouldContain(substring: String): Boolean {
    return this.contains(substring)
}

operator fun Point.plus(other: Point): Point {
    return Point(x + other.x, y + other.y)
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Regular function
        let greet = symbols.iter().find(|s| s.name == "greet");
        assert!(greet.is_some());
        assert_eq!(greet.unwrap().kind, SymbolKind::Function);
        assert!(greet.unwrap().signature.contains("fun greet(name: String): String"));

        // Vararg function with expression body
        let calculate_sum = symbols.iter().find(|s| s.name == "calculateSum");
        assert!(calculate_sum.is_some());
        assert!(calculate_sum.unwrap().signature.contains("vararg numbers: Int"));
        assert!(calculate_sum.unwrap().signature.contains("= numbers.sum()"));

        // Inline reified function
        let is_instance_of = symbols.iter().find(|s| s.name == "isInstanceOf");
        assert!(is_instance_of.is_some());
        assert!(is_instance_of.unwrap().signature.contains("inline fun <reified T>"));

        // Suspend function
        let fetch_data = symbols.iter().find(|s| s.name == "fetchData");
        assert!(fetch_data.is_some());
        assert!(fetch_data.unwrap().signature.contains("suspend fun fetchData"));

        // Extension function on String
        let is_valid_email = symbols.iter().find(|s| s.name == "isValidEmail");
        assert!(is_valid_email.is_some());
        assert!(is_valid_email.unwrap().signature.contains("fun String.isValidEmail()"));

        // Extension function on generic type
        let join_with_commas = symbols.iter().find(|s| s.name == "joinWithCommas");
        assert!(join_with_commas.is_some());
        assert!(join_with_commas.unwrap().signature.contains("fun List<String>.joinWithCommas()"));

        // Generic extension function
        let safe_get = symbols.iter().find(|s| s.name == "safeGet");
        assert!(safe_get.is_some());
        assert!(safe_get.unwrap().signature.contains("fun <T> Collection<T>.safeGet"));

        // Higher-order function
        let process_data = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data.is_some());
        assert!(process_data.unwrap().signature.contains("filter: (String) -> Boolean"));
        assert!(process_data.unwrap().signature.contains("transform: (String) -> String"));

        // Function returning function
        let create_processor = symbols.iter().find(|s| s.name == "createProcessor");
        assert!(create_processor.is_some());
        assert!(create_processor.unwrap().signature.contains("(): (String) -> String"));

        // Tailrec function
        let factorial = symbols.iter().find(|s| s.name == "factorial");
        assert!(factorial.is_some());
        assert!(factorial.unwrap().signature.contains("tailrec fun factorial"));

        // Infix function
        let should_contain = symbols.iter().find(|s| s.name == "shouldContain");
        assert!(should_contain.is_some());
        assert!(should_contain.unwrap().signature.contains("infix fun String.shouldContain"));

        // Operator function
        let plus = symbols.iter().find(|s| s.name == "plus");
        assert!(plus.is_some());
        assert_eq!(plus.unwrap().kind, SymbolKind::Operator);
        assert!(plus.unwrap().signature.contains("operator fun Point.plus"));
    }

    #[test]
    fn test_extract_interfaces_and_delegation() {
        let code = r#"
interface Drawable {
    val color: String
    fun draw()

    fun describe(): String {
        return "Drawing with color $color"
    }
}

interface Clickable {
    fun click() {
        println("Clicked")
    }

    fun showOff() = println("I'm clickable!")
}

class Button(
    private val drawable: Drawable,
    private val clickable: Clickable
) : Drawable by drawable, Clickable by clickable {

    override fun click() {
        println("Button clicked")
        clickable.click()
    }
}

class LazyInitializer {
    val expensiveValue: String by lazy {
        println("Computing expensive value")
        "Expensive computation result"
    }

    var observableProperty: String by Delegates.observable("initial") { prop, old, new ->
        println("Property changed from $old to $new")
    }

    val notNullProperty: String by Delegates.notNull()
}

fun interface StringProcessor {
    fun process(input: String): String
}

fun interface Predicate<T> {
    fun test(item: T): Boolean
}

class ProcessorImpl : StringProcessor {
    override fun process(input: String): String {
        return input.lowercase()
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Interface
        let drawable = symbols.iter().find(|s| s.name == "Drawable");
        assert!(drawable.is_some());
        assert_eq!(drawable.unwrap().kind, SymbolKind::Interface);

        // Abstract property in interface
        let color = symbols.iter().find(|s| s.name == "color" && s.parent_id == drawable.unwrap().id);
        assert!(color.is_some());
        assert!(color.unwrap().signature.contains("val color: String"));

        // Abstract method in interface
        let draw = symbols.iter().find(|s| s.name == "draw" && s.parent_id == drawable.unwrap().id);
        assert!(draw.is_some());
        assert_eq!(draw.unwrap().kind, SymbolKind::Method);

        // Method with default implementation
        let describe = symbols.iter().find(|s| s.name == "describe" && s.parent_id == drawable.unwrap().id);
        assert!(describe.is_some());
        assert!(describe.unwrap().signature.contains("fun describe(): String"));

        // Class with delegation
        let button = symbols.iter().find(|s| s.name == "Button");
        assert!(button.is_some());
        assert!(button.unwrap().signature.contains("Drawable by drawable, Clickable by clickable"));

        // Lazy delegation
        let expensive_value = symbols.iter().find(|s| s.name == "expensiveValue");
        assert!(expensive_value.is_some());
        assert!(expensive_value.unwrap().signature.contains("by lazy"));

        // Observable delegation
        let observable_property = symbols.iter().find(|s| s.name == "observableProperty");
        assert!(observable_property.is_some());
        assert!(observable_property.unwrap().signature.contains("by Delegates.observable"));

        // NotNull delegation
        let not_null_property = symbols.iter().find(|s| s.name == "notNullProperty");
        assert!(not_null_property.is_some());
        assert!(not_null_property.unwrap().signature.contains("by Delegates.notNull()"));

        // Fun interface (SAM interface)
        let string_processor = symbols.iter().find(|s| s.name == "StringProcessor");
        assert!(string_processor.is_some());
        assert!(string_processor.unwrap().signature.contains("fun interface StringProcessor"));

        // Generic fun interface
        let predicate = symbols.iter().find(|s| s.name == "Predicate");
        assert!(predicate.is_some());
        assert!(predicate.unwrap().signature.contains("fun interface Predicate<T>"));
    }

    #[test]
    fn test_extract_annotations_and_type_aliases() {
        let code = r#"
@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class MyAnnotation(
    val value: String,
    val priority: Int = 0
)

@Repeatable
@Target(AnnotationTarget.PROPERTY)
annotation class Author(val name: String)

typealias StringProcessor = (String) -> String
typealias UserMap = Map<String, User>
typealias Handler<T> = suspend (T) -> Unit

class ProcessingService {
    @MyAnnotation("Important service", priority = 1)
    @Author("John Doe")
    @Author("Jane Smith")
    fun processData(
        @MyAnnotation("Input parameter") input: String
    ): String {
        return input.uppercase()
    }

    @JvmStatic
    @JvmOverloads
    fun createDefault(name: String = "default"): ProcessingService {
        return ProcessingService()
    }
}

@JvmInline
value class UserId(val value: Long)

@JvmInline
value class Email(val address: String) {
    init {
        require(address.contains("@")) { "Invalid email format" }
    }
}

@file:JvmName("UtilityFunctions")
@file:JvmMultifileClass

package com.example.utils

import kotlin.jvm.JvmName
import kotlin.jvm.JvmStatic
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Annotation class
        let my_annotation = symbols.iter().find(|s| s.name == "MyAnnotation");
        assert!(my_annotation.is_some());
        assert!(my_annotation.unwrap().signature.contains("annotation class MyAnnotation"));
        assert!(my_annotation.unwrap().signature.contains("@Target"));
        assert!(my_annotation.unwrap().signature.contains("@Retention"));

        // Repeatable annotation
        let author = symbols.iter().find(|s| s.name == "Author");
        assert!(author.is_some());
        assert!(author.unwrap().signature.contains("@Repeatable"));

        // Type aliases
        let string_processor = symbols.iter().find(|s| s.name == "StringProcessor" && s.kind == SymbolKind::Type);
        assert!(string_processor.is_some());
        assert!(string_processor.unwrap().signature.contains("typealias StringProcessor = (String) -> String"));

        let user_map = symbols.iter().find(|s| s.name == "UserMap");
        assert!(user_map.is_some());
        assert!(user_map.unwrap().signature.contains("typealias UserMap = Map<String, User>"));

        let handler = symbols.iter().find(|s| s.name == "Handler");
        assert!(handler.is_some());
        assert!(handler.unwrap().signature.contains("typealias Handler<T> = suspend (T) -> Unit"));

        // Method with multiple annotations
        let process_data = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data.is_some());
        assert!(process_data.unwrap().signature.contains("@MyAnnotation"));
        assert!(process_data.unwrap().signature.contains("@Author(\"John Doe\")"));
        assert!(process_data.unwrap().signature.contains("@Author(\"Jane Smith\")"));

        // JVM annotations
        let create_default = symbols.iter().find(|s| s.name == "createDefault");
        assert!(create_default.is_some());
        assert!(create_default.unwrap().signature.contains("@JvmStatic"));
        assert!(create_default.unwrap().signature.contains("@JvmOverloads"));

        // Inline value class
        let user_id = symbols.iter().find(|s| s.name == "UserId");
        assert!(user_id.is_some());
        assert!(user_id.unwrap().signature.contains("@JvmInline"));
        assert!(user_id.unwrap().signature.contains("value class UserId"));

        // Value class with validation
        let email = symbols.iter().find(|s| s.name == "Email");
        assert!(email.is_some());
        assert!(email.unwrap().signature.contains("value class Email"));
    }

    #[test]
    fn test_extract_generics_and_variance() {
        let code = r#"
interface Producer<out T> {
    fun produce(): T
}

interface Consumer<in T> {
    fun consume(item: T)
}

interface Processor<T> {
    fun process(input: T): T
}

class Box<T>(private var item: T) {
    fun get(): T = item
    fun set(newItem: T) {
        item = newItem
    }
}

class ContravariantBox<in T> {
    fun put(item: T) {
        // Implementation
    }
}

fun <T : Comparable<T>> findMax(items: List<T>): T? {
    return items.maxOrNull()
}

fun <T> copyWhenGreater(list: List<T>, threshold: T): List<T>
    where T : Comparable<T>, T : Number {
    return list.filter { it > threshold }
}

inline fun <reified T> createArray(size: Int): Array<T?> {
    return arrayOfNulls<T>(size)
}

class Repository<T : Any> {
    private val items = mutableListOf<T>()

    fun add(item: T) {
        items.add(item)
    }

    inline fun <reified R : T> findByType(): List<R> {
        return items.filterIsInstance<R>()
    }
}

fun <K, V> Map<K, V>.getValueOrDefault(key: K, default: () -> V): V {
    return this[key] ?: default()
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Covariant interface
        let producer = symbols.iter().find(|s| s.name == "Producer");
        assert!(producer.is_some());
        assert!(producer.unwrap().signature.contains("interface Producer<out T>"));

        // Contravariant interface
        let consumer = symbols.iter().find(|s| s.name == "Consumer");
        assert!(consumer.is_some());
        assert!(consumer.unwrap().signature.contains("interface Consumer<in T>"));

        // Invariant generic class
        let r#box = symbols.iter().find(|s| s.name == "Box");
        assert!(r#box.is_some());
        assert!(r#box.unwrap().signature.contains("class Box<T>"));

        // Function with type bounds
        let find_max = symbols.iter().find(|s| s.name == "findMax");
        assert!(find_max.is_some());
        assert!(find_max.unwrap().signature.contains("<T : Comparable<T>>"));

        // Function with multiple type constraints
        let copy_when_greater = symbols.iter().find(|s| s.name == "copyWhenGreater");
        assert!(copy_when_greater.is_some());
        assert!(copy_when_greater.unwrap().signature.contains("where T : Comparable<T>, T : Number"));

        // Reified generic function
        let create_array = symbols.iter().find(|s| s.name == "createArray");
        assert!(create_array.is_some());
        assert!(create_array.unwrap().signature.contains("inline fun <reified T>"));

        // Generic class with bounds
        let repository = symbols.iter().find(|s| s.name == "Repository");
        assert!(repository.is_some());
        assert!(repository.unwrap().signature.contains("class Repository<T : Any>"));

        // Extension function on generic type
        let get_value_or_default = symbols.iter().find(|s| s.name == "getValueOrDefault");
        assert!(get_value_or_default.is_some());
        assert!(get_value_or_default.unwrap().signature.contains("fun <K, V> Map<K, V>.getValueOrDefault"));
    }

    #[test]
    fn test_infer_types() {
        let code = r#"
class DataService {
    fun fetchUsers(): List<User> {
        return emptyList()
    }

    suspend fun fetchUserById(id: Long): User? {
        return null
    }

    val cache: MutableMap<String, Any> = mutableMapOf()
    var isEnabled: Boolean = true
}

interface Repository<T> {
    suspend fun findAll(): List<T>
    suspend fun findById(id: Long): T?
}

class UserRepository : Repository<User> {
    override suspend fun findAll(): List<User> {
        return emptyList()
    }

    override suspend fun findById(id: Long): User? {
        return null
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        // Function return types
        let fetch_users = symbols.iter().find(|s| s.name == "fetchUsers");
        assert!(fetch_users.is_some());
        assert_eq!(types.get(&fetch_users.unwrap().id).unwrap(), "List<User>");

        let fetch_user_by_id = symbols.iter().find(|s| s.name == "fetchUserById");
        assert!(fetch_user_by_id.is_some());
        assert_eq!(types.get(&fetch_user_by_id.unwrap().id).unwrap(), "User?");

        // Property types
        let cache = symbols.iter().find(|s| s.name == "cache");
        assert!(cache.is_some());
        assert_eq!(types.get(&cache.unwrap().id).unwrap(), "MutableMap<String, Any>");

        let is_enabled = symbols.iter().find(|s| s.name == "isEnabled");
        assert!(is_enabled.is_some());
        assert_eq!(types.get(&is_enabled.unwrap().id).unwrap(), "Boolean");
    }

    #[test]
    fn test_extract_relationships() {
        let code = r#"
interface Drawable {
    fun draw()
}

interface Clickable {
    fun click()
}

abstract class Widget : Drawable {
    abstract val size: Int
}

class Button : Widget(), Clickable {
    override val size: Int = 100

    override fun draw() {
        println("Drawing button")
    }

    override fun click() {
        println("Button clicked")
    }
}

sealed class State {
    object Loading : State()
    data class Success(val data: String) : State()
    data class Error(val message: String) : State()
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = KotlinExtractor::new(
            "kotlin".to_string(),
            "test.kt".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find inheritance and interface implementation relationships
        assert!(relationships.len() >= 4);

        // Widget implements Drawable
        let widget_drawable = relationships.iter().find(|r| {
            r.kind.to_string() == "implements" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| s.name.as_str()) == Some("Widget") &&
            symbols.iter().find(|s| s.id == r.to_symbol_id).map(|s| s.name.as_str()) == Some("Drawable")
        });
        assert!(widget_drawable.is_some());

        // Button extends Widget
        let button_widget = relationships.iter().find(|r| {
            r.kind.to_string() == "extends" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| s.name.as_str()) == Some("Button") &&
            symbols.iter().find(|s| s.id == r.to_symbol_id).map(|s| s.name.as_str()) == Some("Widget")
        });
        assert!(button_widget.is_some());

        // Button implements Clickable
        let button_clickable = relationships.iter().find(|r| {
            r.kind.to_string() == "implements" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| s.name.as_str()) == Some("Button") &&
            symbols.iter().find(|s| s.id == r.to_symbol_id).map(|s| s.name.as_str()) == Some("Clickable")
        });
        assert!(button_clickable.is_some());

        // Success extends State
        let success_state = relationships.iter().find(|r| {
            r.kind.to_string() == "extends" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| s.name.as_str()) == Some("Success") &&
            symbols.iter().find(|s| s.id == r.to_symbol_id).map(|s| s.name.as_str()) == Some("State")
        });
        assert!(success_state.is_some());
    }
}
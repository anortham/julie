// Implementation of comprehensive Ruby extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

// Submodule declarations
pub mod cross_file_relationships;
pub mod doc_comments;
pub mod extractor;

use crate::extractors::base::{RelationshipKind, SymbolKind, Visibility};
use crate::extractors::ruby::RubyExtractor;
use tree_sitter::Tree;

#[cfg(test)]
mod ruby_extractor_tests {
    use super::*;

    // Helper function to create a RubyExtractor and parse Ruby code
    pub(crate) fn create_extractor_and_parse(code: &str) -> (RubyExtractor, Tree) {
        use std::path::PathBuf;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let extractor =
            RubyExtractor::new("test.rb".to_string(), code.to_string(), &workspace_root);
        (extractor, tree)
    }

    #[test]
    fn test_extract_classes_modules_and_members() {
        let ruby_code = r#"
require 'active_support'
require_relative 'base_model'

module Comparable
  def <=>(other)
    # Implementation
  end

  def between?(min, max)
    self >= min && self <= max
  end
end

module Enumerable
  include Comparable

  def map(&block)
    result = []
    each { |item| result << block.call(item) }
    result
  end
end

class Person
  include Comparable
  extend Enumerable

  attr_reader :name, :age
  attr_writer :email
  attr_accessor :phone

  @@population = 0
  SPECIES = "Homo sapiens"

  def initialize(name, age = 0)
    @name = name
    @age = age
    @@population += 1
  end

  def self.population
    @@population
  end

  def adult?
    @age >= 18
  end

  private

  def secret_method
    "This is private"
  end

  protected

  def family_method
    "This is protected"
  end

  public

  def public_method
    "This is public"
  end
end

class Employee < Person
  def initialize(name, age, salary)
    super(name, age)
    @salary = salary
  end

  def annual_income
    @salary * 12
  end

  alias yearly_income annual_income
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Require statements
        let active_support = symbols.iter().find(|s| s.name == "active_support");
        assert!(active_support.is_some());
        assert_eq!(active_support.unwrap().kind, SymbolKind::Import);

        let base_model = symbols.iter().find(|s| s.name == "base_model");
        assert!(base_model.is_some());
        assert!(
            base_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("require_relative 'base_model'")
        );

        // Module
        let comparable = symbols.iter().find(|s| s.name == "Comparable");
        assert!(comparable.is_some());
        assert_eq!(comparable.unwrap().kind, SymbolKind::Module);

        // Module methods
        let spaceship = symbols.iter().find(|s| s.name == "<=>");
        assert!(spaceship.is_some());
        assert_eq!(spaceship.unwrap().kind, SymbolKind::Method);
        assert_eq!(
            spaceship.unwrap().parent_id,
            Some(comparable.unwrap().id.clone())
        );

        let between = symbols.iter().find(|s| s.name == "between?");
        assert!(between.is_some());
        assert!(
            between
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def between?(min, max)")
        );

        // Module with include
        let enumerable = symbols.iter().find(|s| s.name == "Enumerable");
        assert!(enumerable.is_some());
        assert!(
            enumerable
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Comparable")
        );

        // Class
        let person = symbols.iter().find(|s| s.name == "Person");
        assert!(person.is_some());
        assert_eq!(person.unwrap().kind, SymbolKind::Class);
        assert!(
            person
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Comparable")
        );
        assert!(
            person
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("extend Enumerable")
        );

        // Attribute accessors
        let name_reader = symbols.iter().find(|s| {
            s.name == "name"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("attr_reader"))
        });
        assert!(name_reader.is_some());
        assert_eq!(name_reader.unwrap().kind, SymbolKind::Property);

        let email_writer = symbols.iter().find(|s| {
            s.name == "email"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("attr_writer"))
        });
        assert!(email_writer.is_some());

        let phone_accessor = symbols.iter().find(|s| {
            s.name == "phone"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("attr_accessor"))
        });
        assert!(phone_accessor.is_some());

        // Class variable
        let population = symbols.iter().find(|s| s.name == "@@population");
        assert!(population.is_some());
        assert_eq!(population.unwrap().kind, SymbolKind::Variable);
        assert!(
            population
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@@population = 0")
        );

        // Constant
        let species = symbols.iter().find(|s| s.name == "SPECIES");
        assert!(species.is_some());
        assert_eq!(species.unwrap().kind, SymbolKind::Constant);
        assert!(
            species
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("SPECIES = \"Homo sapiens\"")
        );

        // Constructor
        let initialize = symbols
            .iter()
            .find(|s| s.name == "initialize" && s.parent_id == Some(person.unwrap().id.clone()));
        assert!(initialize.is_some());
        assert_eq!(initialize.unwrap().kind, SymbolKind::Constructor);
        assert!(
            initialize
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def initialize(name, age = 0)")
        );

        // Class method
        let population_method = symbols.iter().find(|s| {
            s.name == "population"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("self."))
        });
        assert!(population_method.is_some());
        assert!(
            population_method
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def self.population")
        );

        // Instance method with question mark
        let adult = symbols.iter().find(|s| s.name == "adult?");
        assert!(adult.is_some());
        assert!(
            adult
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def adult?")
        );

        // Private method
        let secret_method = symbols.iter().find(|s| s.name == "secret_method");
        assert!(secret_method.is_some());
        assert_eq!(
            secret_method.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private
        );

        // Protected method
        let family_method = symbols.iter().find(|s| s.name == "family_method");
        assert!(family_method.is_some());
        assert_eq!(
            family_method.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Protected
        );

        // Public method
        let public_method = symbols.iter().find(|s| s.name == "public_method");
        assert!(public_method.is_some());
        assert_eq!(
            public_method.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );

        // Inheritance
        let employee = symbols.iter().find(|s| s.name == "Employee");
        assert!(employee.is_some());
        assert!(
            employee
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("class Employee < Person")
        );

        // Method alias
        let annual_income = symbols.iter().find(|s| s.name == "annual_income");
        assert!(annual_income.is_some());

        let yearly_income = symbols.iter().find(|s| s.name == "yearly_income");
        assert!(yearly_income.is_some());
        assert!(
            yearly_income
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("alias yearly_income annual_income")
        );
    }

    #[test]
    fn test_extract_metaprogramming_and_dynamic_methods() {
        let ruby_code = r#"
class DynamicClass
  # Define methods dynamically
  ['get', 'set', 'delete'].each do |action|
    define_method("action_data") do |key|
      puts "action data for key"
    end
  end

  # Class-level metaprogramming
  class << self
    def create_accessor(name)
      define_method(name) do
        instance_variable_get("@name")
      end

      define_method("name=") do |value|
        instance_variable_set("@name", value)
      end
    end

    def inherited(subclass)
      puts "subclass inherited from self"
    end
  end

  # Method missing for dynamic behavior
  def method_missing(method_name, *args, &block)
    if method_name.to_s.start_with?('find_by_')
      attribute = method_name.to_s.sub('find_by_', '')
      puts "Finding by attribute with value"
    else
      super
    end
  end

  def respond_to_missing?(method_name, include_private = false)
    method_name.to_s.start_with?('find_by_') || super
  end
end

# Singleton methods
obj = Object.new

def obj.singleton_method
  "I am unique to this object"
end

class << obj
  def another_singleton
    "Another singleton method"
  end
end

# Module refinements
module StringExtensions
  refine String do
    def palindrome?
      self == self.reverse
    end

    def word_count
      self.split.size
    end
  end
end

class TextProcessor
  using StringExtensions

  def process(text)
    puts "Palindrome: result"
    puts "Word count: result"
  end
end

# Eval methods
class EvalExample
  def class_eval_example
    self.class.class_eval do
      def dynamic_method
        "Created with class eval"
      end
    end
  end

  def instance_eval_example
    instance_eval do
      @dynamic_var = "Created with instance eval"
    end
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Dynamic class
        let dynamic_class = symbols.iter().find(|s| s.name == "DynamicClass");
        assert!(dynamic_class.is_some());

        // Dynamically defined methods (should extract the define_method call)
        let define_method_call = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("define_method"))
        });
        assert!(define_method_call.is_some());

        // Singleton class
        let singleton_class = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("class << self"))
        });
        assert!(singleton_class.is_some());

        // method_missing
        let method_missing = symbols.iter().find(|s| s.name == "method_missing");
        assert!(method_missing.is_some());
        assert!(
            method_missing
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def method_missing(method_name, *args, &block)")
        );

        // respond_to_missing?
        let respond_to_missing = symbols.iter().find(|s| s.name == "respond_to_missing?");
        assert!(respond_to_missing.is_some());

        // Singleton method on object
        let singleton_method = symbols.iter().find(|s| s.name == "singleton_method");
        assert!(singleton_method.is_some());
        assert!(
            singleton_method
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def obj.singleton_method")
        );

        // Module refinement
        let string_extensions = symbols.iter().find(|s| s.name == "StringExtensions");
        assert!(string_extensions.is_some());

        // Refined method
        let palindrome = symbols.iter().find(|s| s.name == "palindrome?");
        assert!(palindrome.is_some());

        // Using directive
        let text_processor = symbols.iter().find(|s| s.name == "TextProcessor");
        assert!(text_processor.is_some());
        assert!(
            text_processor
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("using StringExtensions")
        );

        // Eval methods
        let class_eval_example = symbols.iter().find(|s| s.name == "class_eval_example");
        assert!(class_eval_example.is_some());

        let instance_eval_example = symbols.iter().find(|s| s.name == "instance_eval_example");
        assert!(instance_eval_example.is_some());
    }

    #[test]
    fn test_extract_blocks_procs_and_lambdas() {
        let ruby_code = r#"
class BlockProcessor
  def initialize
    @callbacks = []
  end

  def process_with_block(&block)
    result = yield if block_given?
    puts "Block result: result"
  end

  def each_item(items, &block)
    items.each do |item|
      block.call(item)
    end
  end

  def filter_items(items, &predicate)
    items.select(&predicate)
  end

  # Method that returns a proc
  def create_multiplier(factor)
    proc { |x| x * factor }
  end

  # Method that returns a lambda
  def create_validator(min, max)
    lambda { |value| value.between?(min, max) }
  end

  # Method with block parameter
  def transform_data(data, transformer: nil, &block)
    processor = transformer || block
    data.map(&processor)
  end
end

# Various ways to create procs and lambdas
doubler = proc { |x| x * 2 }
tripler = Proc.new { |x| x * 3 }
validator = lambda { |x| x > 0 }
shorthand_lambda = ->(x) { x.upcase }

# Block usage examples
numbers = [1, 2, 3, 4, 5]

# Block with each
numbers.each do |num|
  puts num * 2
end

# Block with map
squared = numbers.map { |n| n ** 2 }

# Block with select
evens = numbers.select(&:even?)

# Proc and lambda assignments
add_one = -> (x) { x + 1 }
multiply = proc { |a, b| a * b }

class EventHandler
  def initialize
    @on_success = nil
    @on_error = nil
  end

  def on_success(&block)
    @on_success = block
  end

  def on_error(&block)
    @on_error = block
  end

  def trigger_success(data)
    @on_success&.call(data)
  end

  def trigger_error(error)
    @on_error&.call(error)
  end
end

# Method that takes multiple block types
def flexible_processor(data, &block)
  case block.arity
  when 1
    data.map(&block)
  when 2
    data.each_with_index.map(&block)
  else
    data
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Class with block methods
        let block_processor = symbols.iter().find(|s| s.name == "BlockProcessor");
        assert!(block_processor.is_some());

        // Method with block parameter
        let process_with_block = symbols.iter().find(|s| s.name == "process_with_block");
        assert!(process_with_block.is_some());
        assert!(
            process_with_block
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("(&block)")
        );

        let each_item = symbols.iter().find(|s| s.name == "each_item");
        assert!(each_item.is_some());
        assert!(
            each_item
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("(items, &block)")
        );

        // Method returning proc
        let create_multiplier = symbols.iter().find(|s| s.name == "create_multiplier");
        assert!(create_multiplier.is_some());

        // Method returning lambda
        let create_validator = symbols.iter().find(|s| s.name == "create_validator");
        assert!(create_validator.is_some());

        // Method with named block parameter
        let transform_data = symbols.iter().find(|s| s.name == "transform_data");
        assert!(transform_data.is_some());
        assert!(
            transform_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("transformer: nil, &block")
        );

        // Proc assignments
        let doubler = symbols.iter().find(|s| s.name == "doubler");
        assert!(doubler.is_some());
        assert!(
            doubler
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("proc { |x| x * 2 }")
        );

        let tripler = symbols.iter().find(|s| s.name == "tripler");
        assert!(tripler.is_some());
        assert!(
            tripler
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Proc.new")
        );

        // Lambda assignments
        let validator = symbols.iter().find(|s| s.name == "validator");
        assert!(validator.is_some());
        assert!(
            validator
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("lambda")
        );

        let shorthand_lambda = symbols.iter().find(|s| s.name == "shorthand_lambda");
        assert!(shorthand_lambda.is_some());
        assert!(
            shorthand_lambda
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("->(x)")
        );

        // Event handler with callbacks
        let event_handler = symbols.iter().find(|s| s.name == "EventHandler");
        assert!(event_handler.is_some());

        let on_success = symbols.iter().find(|s| s.name == "on_success");
        assert!(on_success.is_some());

        let trigger_success = symbols.iter().find(|s| s.name == "trigger_success");
        assert!(trigger_success.is_some());

        // Flexible processor
        let flexible_processor = symbols.iter().find(|s| s.name == "flexible_processor");
        assert!(flexible_processor.is_some());
    }

    #[test]
    fn test_extract_constants_symbols_and_variables() {
        let ruby_code = r#"
module Constants
  # Various constant types
  PI = 3.14159
  APP_NAME = "My Application"
  VERSION = [1, 2, 3]

  # Nested constants
  module Database
    HOST = "localhost"
    PORT = 5432
    CONFIG = {
      host: HOST,
      port: PORT,
      adapter: "postgresql"
    }
  end

  # Class constants
  class User
    DEFAULT_ROLE = :user
    VALID_STATUSES = [:active, :inactive, :pending]
    MAX_LOGIN_ATTEMPTS = 3

    def initialize(name)
      @name = name
      @login_attempts = 0
      @@user_count ||= 0
      @@user_count += 1
    end

    def self.user_count
      @@user_count
    end
  end
end

# Symbol usage
status_symbols = [:pending, :approved, :rejected]
method_name = :calculate_total
hash_with_symbols = {
  name: "John",
  age: 30,
  active: true
}

# Different variable types
$global_variable = "I'm global"
@instance_variable = "I'm an instance variable"
@@class_variable = "I'm a class variable"
local_variable = "I'm local"
CONSTANT_VARIABLE = "I'm a constant"

class VariableExample
  def initialize
    @instance_var = "instance"
    @@class_var = "class"
  end

  def self.class_method
    @@class_var
  end

  def instance_method
    local_var = "local"
    @instance_var
  end

  # Constant within class
  INNER_CONSTANT = "inner"
end

# Parallel assignment
a, b, c = 1, 2, 3
first, *rest = [1, 2, 3, 4, 5]
x, y = y, x  # swap

# Multiple assignment with methods
def multiple_return
  return 1, 2, 3
end

one, two, three = multiple_return

# Constants with special characters
class HTTPClient
  DEFAULT_TIMEOUT = 30
  MAX_RETRIES = 3
  BASE_URL = "https://api.example.com"
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Module constants
        let pi = symbols.iter().find(|s| s.name == "PI");
        assert!(pi.is_some());
        assert_eq!(pi.unwrap().kind, SymbolKind::Constant);
        assert!(
            pi.unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("PI = 3.14159")
        );

        let app_name = symbols.iter().find(|s| s.name == "APP_NAME");
        assert!(app_name.is_some());
        assert!(
            app_name
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("\"My Application\"")
        );

        // Nested module
        let database = symbols.iter().find(|s| s.name == "Database");
        assert!(database.is_some());
        assert_eq!(database.unwrap().kind, SymbolKind::Module);

        // Nested constants
        let host = symbols.iter().find(|s| s.name == "HOST");
        assert!(host.is_some());
        assert_eq!(host.unwrap().parent_id, Some(database.unwrap().id.clone()));

        let config = symbols.iter().find(|s| s.name == "CONFIG");
        assert!(config.is_some());
        assert!(
            config
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CONFIG = {")
        );

        // Class with constants
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());

        let default_role = symbols.iter().find(|s| s.name == "DEFAULT_ROLE");
        assert!(default_role.is_some());
        assert!(
            default_role
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(":user")
        );

        let valid_statuses = symbols.iter().find(|s| s.name == "VALID_STATUSES");
        assert!(valid_statuses.is_some());
        assert!(
            valid_statuses
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("[:active, :inactive, :pending]")
        );

        // Instance variables
        let name_var = symbols.iter().find(|s| s.name == "@name");
        assert!(name_var.is_some());
        assert_eq!(name_var.unwrap().kind, SymbolKind::Variable);

        // Class variables
        let user_count = symbols.iter().find(|s| s.name == "@@user_count");
        assert!(user_count.is_some());
        assert_eq!(user_count.unwrap().kind, SymbolKind::Variable);

        // Global variable
        let global_var = symbols.iter().find(|s| s.name == "$global_variable");
        assert!(global_var.is_some());
        assert!(
            global_var
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("$global_variable = \"I'm global\"")
        );

        // Symbol assignments
        let status_symbols = symbols.iter().find(|s| s.name == "status_symbols");
        assert!(status_symbols.is_some());
        assert!(
            status_symbols
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("[:pending, :approved, :rejected]")
        );

        let method_name = symbols.iter().find(|s| s.name == "method_name");
        assert!(method_name.is_some());
        assert!(
            method_name
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(":calculate_total")
        );

        // Hash with symbols
        let hash_with_symbols = symbols.iter().find(|s| s.name == "hash_with_symbols");
        assert!(hash_with_symbols.is_some());
        assert!(
            hash_with_symbols
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("name: \"John\"")
        );

        // Parallel assignment
        let parallel_a = symbols.iter().find(|s| {
            s.name == "a"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("a, b, c = 1, 2, 3"))
        });
        assert!(parallel_a.is_some());

        let rest = symbols.iter().find(|s| s.name == "rest");
        assert!(rest.is_some());
        assert!(
            rest.unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("first, *rest = [1, 2, 3, 4, 5]")
        );

        // Multiple return
        let multiple_return = symbols.iter().find(|s| s.name == "multiple_return");
        assert!(multiple_return.is_some());
        let multiple_return_signature = multiple_return.unwrap().signature.as_ref().unwrap();
        assert!(multiple_return_signature.contains("return 1, 2, 3"));
    }

    #[test]
    fn test_function_signature_includes_return_statement() {
        let ruby_code = r#"
def simple_return
  return "hello"
end

def multiple_return
  return 1, 2, 3
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        let simple_return = symbols.iter().find(|s| s.name == "simple_return");
        assert!(simple_return.is_some());
        let simple_signature = simple_return.unwrap().signature.as_ref().unwrap();
        assert!(
            simple_signature.contains("return \"hello\""),
            "Expected signature '{}' to contain 'return \"hello\"'",
            simple_signature
        );

        let multiple_return = symbols.iter().find(|s| s.name == "multiple_return");
        assert!(multiple_return.is_some());
        let multiple_signature = multiple_return.unwrap().signature.as_ref().unwrap();
        assert!(
            multiple_signature.contains("return 1, 2, 3"),
            "Expected signature '{}' to contain 'return 1, 2, 3'",
            multiple_signature
        );
    }

    #[test]
    fn test_extract_mixins_and_module_inclusion() {
        let ruby_code = r#"
module Loggable
  def log(message)
    puts "[LOG] #{message}"
  end

  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    def log_class_info
      puts "Class: #{self.name}"
    end
  end
end

module Cacheable
  def cache_key
    "cache_key_id"
  end

  def cached?
    Cache.exists?(cache_key)
  end
end

module Timestampable
  def self.prepended(base)
    base.class_eval do
      attr_accessor :created_at, :updated_at
    end
  end

  def touch
    self.updated_at = Time.now
  end
end

class BaseModel
  include Loggable
  include Cacheable
  prepend Timestampable

  attr_reader :id

  def initialize(id)
    @id = id
    @created_at = Time.now
    @updated_at = Time.now
  end
end

class User < BaseModel
  extend Forwardable

  def_delegator :@profile, :email
  def_delegator :@profile, :name, :full_name

  def initialize(id, profile)
    super(id)
    @profile = profile
  end
end

# Module as namespace
module API
  module V1
    class UsersController
      include Loggable

      def index
        log "Fetching all users"
        # Implementation
      end
    end
  end

  module V2
    class UsersController
      include Loggable

      def index
        log "Fetching all users (v2)"
        # Implementation
      end
    end
  end
end

# Multiple inclusion patterns
class ComplexModel
  include Enumerable
  include Comparable
  extend Forwardable

  def initialize(items)
    @items = items
  end

  def each(&block)
    @items.each(&block)
  end

  def <=>(other)
    @items.size <=> other.items.size
  end

  protected

  attr_reader :items
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Module with callback
        let loggable = symbols.iter().find(|s| s.name == "Loggable");
        assert!(loggable.is_some());
        assert_eq!(loggable.unwrap().kind, SymbolKind::Module);

        let included = symbols.iter().find(|s| s.name == "included");
        assert!(included.is_some());
        assert!(
            included
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def self.included(base)")
        );

        // Nested module
        let class_methods = symbols
            .iter()
            .find(|s| s.name == "ClassMethods" && s.kind == SymbolKind::Module);
        assert!(class_methods.is_some());
        assert_eq!(
            class_methods.unwrap().parent_id,
            Some(loggable.unwrap().id.clone())
        );

        // Module with prepend callback
        let timestampable = symbols.iter().find(|s| s.name == "Timestampable");
        assert!(timestampable.is_some());

        let prepended = symbols.iter().find(|s| s.name == "prepended");
        assert!(prepended.is_some());
        assert!(
            prepended
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("def self.prepended(base)")
        );

        // Class with multiple inclusions
        let base_model = symbols.iter().find(|s| s.name == "BaseModel");
        assert!(base_model.is_some());
        assert!(
            base_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Loggable")
        );
        assert!(
            base_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Cacheable")
        );
        assert!(
            base_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("prepend Timestampable")
        );

        // Class with extension
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(
            user.unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("extend Forwardable")
        );

        // Delegation
        let def_delegator = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("def_delegator"))
        });
        assert!(def_delegator.is_some());

        // Nested namespace modules
        let api_v1 = symbols.iter().find(|s| s.name == "V1");
        assert!(api_v1.is_some());

        let api_v2 = symbols.iter().find(|s| s.name == "V2");
        assert!(api_v2.is_some());

        let v1_controller = symbols.iter().find(|s| {
            s.name == "UsersController"
                && s.signature.as_ref().map_or(false, |sig| sig.contains("V1"))
        });
        assert!(v1_controller.is_some());

        let v2_controller = symbols.iter().find(|s| {
            s.name == "UsersController"
                && s.signature.as_ref().map_or(false, |sig| sig.contains("V2"))
        });
        assert!(v2_controller.is_some());

        // Complex model with multiple mixins
        let complex_model = symbols.iter().find(|s| s.name == "ComplexModel");
        assert!(complex_model.is_some());
        assert!(
            complex_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Enumerable")
        );
        assert!(
            complex_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("include Comparable")
        );
        assert!(
            complex_model
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("extend Forwardable")
        );
    }

    #[test]
    fn test_infer_basic_types_from_assignments() {
        let ruby_code = r#"
class Calculator
  def add(a, b)
    a + b
  end

  def multiply_by_two(number)
    number * 2
  end

  def get_user_name
    "John Doe"
  end

  def get_numbers
    [1, 2, 3, 4, 5]
  end

  def get_config
    { host: "localhost", port: 3000 }
  end

  def is_valid?
    true
  end

  attr_reader :result
  attr_writer :factor
  attr_accessor :mode

  def initialize
    @result = 0
    @factor = 1.0
    @mode = :automatic
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Note: Ruby type inference is much more limited due to dynamic nature
        // We can infer some basic types from literals and patterns

        let get_user_name = symbols.iter().find(|s| s.name == "get_user_name");
        assert!(get_user_name.is_some());
        // Ruby type inference would be limited, but we might infer String from return value

        let get_numbers = symbols.iter().find(|s| s.name == "get_numbers");
        assert!(get_numbers.is_some());
        // Might infer Array from return value

        let is_valid = symbols.iter().find(|s| s.name == "is_valid?");
        assert!(is_valid.is_some());
        // Might infer Boolean from method name pattern and return value

        // Instance variables with initial values
        let result_symbol = symbols.iter().find(|s| s.name == "@result");
        assert!(result_symbol.is_some());
        // Might infer Integer from assignment

        let factor = symbols.iter().find(|s| s.name == "@factor");
        assert!(factor.is_some());
        // Might infer Float from assignment

        let mode = symbols.iter().find(|s| s.name == "@mode");
        assert!(mode.is_some());
        // Might infer Symbol from assignment
    }

    #[test]
    fn test_extract_inheritance_and_module_relationships() {
        let ruby_code = r#"
module Drawable
  def draw
    puts "Drawing"
  end
end

module Colorable
  def set_color(color)
    @color = color
  end
end

module Comparable
  def <=>(other)
    # Implementation
  end
end

class Shape
  include Drawable
  extend Colorable

  def initialize
    @color = :black
  end
end

class Circle < Shape
  include Comparable

  def initialize(radius)
    super()
    @radius = radius
  end

  def area
    Math::PI * @radius ** 2
  end

  def <=>(other)
    area <=> other.area
  end
end

class Rectangle < Shape
  def initialize(width, height)
    super()
    @width = width
    @height = height
  end

  def area
    @width * @height
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find inheritance and module inclusion relationships
        assert!(relationships.len() >= 4);

        // Circle extends Shape
        let circle_shape = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Circle")
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Shape")
        });
        assert!(circle_shape.is_some());

        // Rectangle extends Shape
        let rectangle_shape = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Rectangle")
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Shape")
        });
        assert!(rectangle_shape.is_some());

        // Shape includes Drawable
        let shape_drawable = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Implements
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Shape")
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Drawable")
        });
        assert!(shape_drawable.is_some());

        // Circle includes Comparable
        let circle_comparable = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Implements
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Circle")
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .map(|s| s.name.as_str())
                    == Some("Comparable")
        });
        assert!(circle_comparable.is_some());
    }

    #[test]
    fn test_no_duplicate_assignment_symbols() {
        // REGRESSION TEST: Ensure is_part_of_assignment guard prevents duplicate symbols
        // Without this guard, assignments like @foo = 42 create TWO symbols:
        // 1. One from the assignment node
        // 2. One from the instance_variable node itself
        // With the guard, only the assignment creates the symbol (correct behavior)
        let ruby_code = r#"
class Example
  def initialize
    @foo = 42
    @bar = "hello"
    @@class_var = 100
    $global_var = "world"
  end

  def update
    @foo = 99  # Another assignment to same variable
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Count occurrences of each variable
        let foo_count = symbols.iter().filter(|s| s.name == "@foo").count();
        let bar_count = symbols.iter().filter(|s| s.name == "@bar").count();
        let class_var_count = symbols.iter().filter(|s| s.name == "@@class_var").count();
        let global_var_count = symbols.iter().filter(|s| s.name == "$global_var").count();

        // Each variable should appear EXACTLY the number of times it's assigned
        // @foo is assigned twice (initialize and update), so should appear twice
        // Others are assigned once, so should appear once
        assert_eq!(
            foo_count, 2,
            "Instance variable @foo should appear exactly twice (two assignments)"
        );
        assert_eq!(
            bar_count, 1,
            "Instance variable @bar should appear exactly once (no duplicates)"
        );
        assert_eq!(
            class_var_count, 1,
            "Class variable @@class_var should appear exactly once (no duplicates)"
        );
        assert_eq!(
            global_var_count, 1,
            "Global variable $global_var should appear exactly once (no duplicates)"
        );

        // Verify they are all Variable kind
        let foo = symbols.iter().find(|s| s.name == "@foo");
        assert!(foo.is_some());
        assert_eq!(foo.unwrap().kind, SymbolKind::Variable);

        let bar = symbols.iter().find(|s| s.name == "@bar");
        assert!(bar.is_some());
        assert_eq!(bar.unwrap().kind, SymbolKind::Variable);
    }
}

// ========================================================================
// Identifier Extraction Tests (TDD RED phase)
// ========================================================================

#[cfg(test)]
mod identifier_extraction_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use super::ruby_extractor_tests::create_extractor_and_parse;
    use super::*;
    use crate::extractors::base::IdentifierKind;

    #[test]
    fn test_extract_function_calls() {
        let ruby_code = r#"
class Calculator
  def add(a, b)
    a + b
  end

  def calculate
    result = add(5, 3)      # Function call to add
    puts result             # Function call to puts
    result
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);

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

        let puts_call = identifiers.iter().find(|id| id.name == "puts");
        assert!(
            puts_call.is_some(),
            "Should extract 'puts' function call identifier"
        );
        let puts_call = puts_call.unwrap();
        assert_eq!(puts_call.kind, IdentifierKind::Call);

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
        let ruby_code = r#"
class User
  attr_accessor :name, :email

  def print_info
    puts self.name          # Member access: self.name
    email_value = self.email # Member access: self.email
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);

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
        // Critical bug fix from Rust implementation
        let ruby_code = r#"
class Service
  def process
    helper()            # Call to helper in same file (with parens)
  end

  def helper
    # Helper method
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);

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
        let ruby_code = r#"
class DataService
  def execute
    result = user.account.balance   # Chained member access
    name = customer.profile.name     # Chained member access
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);

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
        let ruby_code = r#"
class Test
  def run
    process()
    process()  # Same call twice
  end

  def process
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);

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
}

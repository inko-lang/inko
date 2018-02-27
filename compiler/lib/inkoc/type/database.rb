# frozen_string_literal: true

module Inkoc
  module Type
    class Database
      include Inspect

      attr_reader :top_level, :true_type, :false_type, :nil_type, :block_type,
                  :integer_type, :float_type, :string_type, :array_type,
                  :hash_map_type, :void_type, :file_type,
                  :object_type, :hasher_type

      def initialize
        @object_type = Object.new(name: Config::OBJECT_CONST)
        @top_level = new_object_type('Inko')

        @true_type = Type::Boolean.new(name: Config::TRUE_CONST)
        @false_type = Type::Boolean.new(name: Config::FALSE_CONST)
        @nil_type = Nil.new(prototype: object_type)

        @block_type = new_object_type(Config::BLOCK_CONST)
        @integer_type = new_object_type(Config::INTEGER_CONST, singleton: true)
        @float_type = new_object_type(Config::FLOAT_CONST, singleton: true)
        @string_type = new_object_type(Config::STRING_CONST, singleton: true)
        @file_type = new_object_type(Config::FILE_CONST)
        @hasher_type = new_object_type(Config::HASHER_CONST)
        @array_type = initialize_array_type
        @hash_map_type = initialize_hash_map_type

        @void_type = Void.new
        @trait_type = nil
      end

      def trait_type
        @trait_type ||= top_level.type_of_attribute(Config::TRAIT_CONST)
      end

      def boolean_type
        @boolean_type ||= top_level.type_of_attribute(Config::BOOLEAN_CONST)
      end

      def initialize_array_type
        type = new_object_type(Config::ARRAY_CONST)

        type.define_type_parameter(Config::ARRAY_TYPE_PARAMETER)

        type
      end

      def initialize_hash_map_type
        type = new_object_type(Config::HASH_MAP_CONST)

        type.define_type_parameter(Config::HASH_MAP_KEY_TYPE_PARAMETER)
        type.define_type_parameter(Config::HASH_MAP_VALUE_TYPE_PARAMETER)

        type
      end

      def new_array_of_type(type)
        array = array_type.new_shallow_instance
        array.initialize_type_parameter(Config::ARRAY_TYPE_PARAMETER, type)

        array
      end

      def new_object_type(name, proto = object_type, singleton: false)
        Object.new(name: name, prototype: proto, singleton: singleton)
      end

      def new_empty_object
        Object.new
      end
    end
  end
end

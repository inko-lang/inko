# frozen_string_literal: true

module Inkoc
  module Type
    class Database
      include Inspect

      attr_reader :top_level, :true_type, :false_type, :nil_type, :block_type,
                  :integer_type, :float_type, :string_type, :array_type,
                  :hash_map_type, :void_type

      def initialize
        @top_level = Object.new(name: 'Inko')
        @true_type = Object.new(name: Config::TRUE_CONST)
        @false_type = Object.new(name: Config::FALSE_CONST)
        @nil_type = Object.new(name: Config::NIL_CONST)
        @block_type = Object.new(name: Config::BLOCK_CONST)
        @integer_type = Object.new(name: Config::INTEGER_CONST)
        @float_type = Object.new(name: Config::FLOAT_CONST)
        @string_type = Object.new(name: Config::STRING_CONST)
        @array_type = initialize_array_type
        @hash_map_type = initialize_hash_map_type
        @void_type = Void.new

        @trait_type = nil
        @object_type = nil
      end

      def object_type
        @object_type ||= top_level.type_of_attribute(Config::OBJECT_CONST)
      end

      def trait_type
        @trait_type ||= top_level.type_of_attribute(Config::TRAIT_CONST)
      end

      def initialize_array_type
        type = Object.new(name: Config::ARRAY_CONST)
        param = Type::Trait
          .new(name: Config::ARRAY_TYPE_PARAMETER, generated: true)

        type.define_type_parameter(param.name, param)

        type
      end

      def initialize_hash_map_type
        type = Object.new(name: Config::HASH_MAP_CONST)
        key = Type::Trait
          .new(name: Config::HASH_MAP_KEY_TYPE_PARAMETER, generated: true)

        val = Type::Trait
          .new(name: Config::HASH_MAP_VALUE_TYPE_PARAMETER, generated: true)

        type.define_type_parameter(key.name, key)
        type.define_type_parameter(val.name, val)

        type
      end

      def new_array_of_type(type)
        array_type.new_instance(Config::ARRAY_TYPE_PARAMETER => type)
      end
    end
  end
end

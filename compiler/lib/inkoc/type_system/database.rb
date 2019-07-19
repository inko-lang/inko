# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Database
      attr_reader :top_level, :true_type, :false_type, :nil_type, :block_type,
                  :integer_type, :float_type, :string_type, :array_type,
                  :object_type, :hasher_type, :boolean_type, :file_type,
                  :byte_array_type, :library_type, :function_type,
                  :pointer_type, :process_type, :socket_type, :unix_socket_type

      def initialize
        @object_type = new_object_type(Config::OBJECT_CONST, nil)
        @top_level = new_object_type(Config::INKO_CONST)
        @boolean_type = new_object_type(Config::BOOLEAN_CONST)
        @true_type = @boolean_type.new_instance
        @false_type = @boolean_type.new_instance
        @nil_type = new_object_type(Config::NIL_CONST)
        @block_type = new_object_type(Config::BLOCK_CONST)
        @integer_type = new_object_type(Config::INTEGER_CONST)
        @float_type = new_object_type(Config::FLOAT_CONST)
        @string_type = new_object_type(Config::STRING_CONST)
        @file_type = new_object_type(Config::FILE_CONST)
        @hasher_type = new_object_type(Config::HASHER_CONST)
        @byte_array_type = new_object_type(Config::BYTE_ARRAY_CONST)
        @array_type = initialize_array_type
        @library_type = new_object_type(Config::LIBRARY_CONST)
        @function_type = new_object_type(Config::FUNCTION_CONST)
        @pointer_type = new_object_type(Config::POINTER_CONST)
        @process_type = new_object_type(Config::PROCESS_CONST)
        @socket_type = new_object_type(Config::SOCKET_CONST)
        @unix_socket_type = new_object_type(Config::UNIX_SOCKET_CONST)
        @trait_id = -1
      end

      def trait_type
        top_level.lookup_attribute(Config::TRAIT_CONST).type
      end

      def new_array_of_type(type)
        array_type.new_instance([type])
      end

      def new_object_type(name, proto = object_type)
        Object.new(name: name, prototype: proto)
      end

      def new_empty_object(prototype = object_type)
        Object.new(prototype: prototype)
      end

      def new_trait_type(name, proto = trait_type)
        Trait.new(name: name, prototype: proto, unique_id: @trait_id += 1)
      end

      def initialize_array_type
        new_object_type(Config::ARRAY_CONST).tap do |array|
          array.define_type_parameter(Config::ARRAY_TYPE_PARAMETER)
        end
      end
    end
  end
end

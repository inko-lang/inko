# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Database
      attr_reader :true_type, :false_type, :nil_type, :block_type,
                  :integer_type, :float_type, :string_type, :array_type,
                  :object_type, :boolean_type, :file_type, :byte_array_type,
                  :module_type, :ffi_library_type, :ffi_function_type,
                  :ffi_pointer_type, :ip_socket_type, :unix_socket_type,
                  :process_type, :read_only_file_type, :write_only_file_type,
                  :read_write_file_type, :hasher_type

      def initialize
        @object_type = new_object_type(Config::OBJECT_CONST, nil)
        @boolean_type = new_object_type(Config::BOOLEAN_CONST)
        @true_type = @boolean_type.new_instance
        @false_type = @boolean_type.new_instance
        @nil_type = new_object_type(Config::NIL_CONST)
        @block_type = new_object_type(Config::BLOCK_CONST)
        @integer_type = new_object_type(Config::INTEGER_CONST)
        @float_type = new_object_type(Config::FLOAT_CONST)
        @string_type = new_object_type(Config::STRING_CONST)
        @file_type = new_object_type(Config::FILE_CONST)
        @byte_array_type = new_object_type(Config::BYTE_ARRAY_CONST)
        @array_type = initialize_array_type
        @module_type = new_object_type(Config::MODULE_TYPE)
        @ffi_library_type = new_object_type(Config::FFI_LIBRARY_TYPE)
        @ffi_function_type = new_object_type(Config::FFI_FUNCTION_TYPE)
        @ffi_pointer_type = new_object_type(Config::FFI_POINTER_TYPE)
        @ip_socket_type = new_object_type(Config::IP_SOCKET_TYPE)
        @unix_socket_type = new_object_type(Config::UNIX_SOCKET_TYPE)
        @process_type = new_object_type(Config::PROCESS_TYPE)
        @read_only_file_type = new_object_type(Config::READ_ONLY_FILE_TYPE)
        @write_only_file_type = new_object_type(Config::WRITE_ONLY_FILE_TYPE)
        @read_write_file_type = new_object_type(Config::READ_WRITE_FILE_TYPE)
        @hasher_type = new_object_type(Config::HASHER_TYPE)
        @trait_id = -1
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

      def new_trait_type(name, proto = nil)
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

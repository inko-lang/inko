module Inko
  class Instruction
    attr_reader :name, :arguments, :line, :column

    NAME_MAPPING = [
      :set_integer,
      :set_float,
      :set_string,
      :set_object,
      :set_array,
      :get_integer_prototype,
      :get_float_prototype,
      :get_string_prototype,
      :get_array_prototype,
      :get_true_prototype,
      :get_false_prototype,
      :get_method_prototype,
      :get_compiled_code_prototype,
      :get_true,
      :get_false,
      :set_local,
      :get_local,
      :set_literal_const,
      :get_literal_const,
      :set_literal_attr,
      :get_literal_attr,
      :set_compiled_code,
      :send_literal,
      :return,
      :goto_if_false,
      :goto_if_true,
      :goto,
      :def_method,
      :def_literal_method,
      :run_code,
      :get_toplevel,
      :is_error,
      :integer_add,
      :integer_div,
      :integer_mul,
      :integer_sub,
      :integer_mod,
      :integer_to_float,
      :integer_to_string,
      :integer_bitwise_and,
      :integer_bitwise_or,
      :integer_bitwise_xor,
      :integer_shift_left,
      :integer_shift_right,
      :integer_smaller,
      :integer_greater,
      :integer_equals,
      :spawn_literal_process,
      :float_add,
      :float_mul,
      :float_div,
      :float_sub,
      :float_mod,
      :float_to_integer,
      :float_to_string,
      :float_smaller,
      :float_greater,
      :float_equals,
      :array_insert,
      :array_at,
      :array_remove,
      :array_length,
      :array_clear,
      :string_to_lower,
      :string_to_upper,
      :string_equals,
      :string_to_bytes,
      :string_from_bytes,
      :string_length,
      :string_size,
      :stdout_write,
      :stderr_write,
      :stdin_read,
      :stdin_read_line,
      :file_open,
      :file_write,
      :file_read,
      :file_read_line,
      :file_flush,
      :file_size,
      :file_seek,
      :run_literal_file,
      :run_file,
      :send,
      :get_self,
      :get_binding_prototype,
      :get_binding,
      :set_const,
      :get_const,
      :set_attr,
      :get_attr,
      :literal_const_exists,
      :run_literal_code,
      :set_prototype,
      :get_prototype,
      :local_exists,
      :get_caller,
      :literal_responds_to,
      :responds_to,
      :literal_attr_exists,
      :set_outer_scope,
      :spawn_process,
      :send_process_message,
      :receive_process_message,
      :get_current_pid,
      :set_parent_local,
      :get_parent_local,
      :get_binding_of_caller
    ].each_with_index.each_with_object({}) do |(value, index), hash|
      hash[value] = index
    end

    # Instructions where the register containing a value to return is the last
    # register, instead of the first one.
    REGISTER_LAST = [
      :set_const,
      :set_literal_const,
      :set_attr,
      :set_literal_attr,
      :set_local,
      :set_parent_local
    ]

    def initialize(name, arguments, line, column)
      @name = name
      @arguments = arguments
      @line = line
      @column = column

      if !line or !column
        raise ArgumentError, 'A line and column number are required'
      end

      @arguments.each do |arg|
        unless arg.is_a?(Fixnum)
          raise TypeError, "arguments must be Fixnums, not a #{arg.class}"
        end
      end
    end

    def name_integer
      NAME_MAPPING.fetch(name)
    end

    def remap_arguments
      @arguments.map! { |arg| yield arg }
    end

    def written_register
      if REGISTER_LAST.include?(name.to_sym)
        arguments[-1]
      else
        arguments[0]
      end
    end

    def inspect
      "Instruction(#{name}, #{arguments.inspect}, line: #{line}, column: #{column})"
    end
  end
end

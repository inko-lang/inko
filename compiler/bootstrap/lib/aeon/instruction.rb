module Aeon
  class Instruction
    attr_reader :name, :arguments, :line, :column

    NAME_MAPPING = {
      set_integer: 0,
      set_float: 1,
      set_string: 2,
      set_object: 3,
      set_array: 4,
      get_integer_prototype: 6,
      get_float_prototype: 7,
      get_string_prototype: 8,
      get_array_prototype: 9,
      get_thread_prototype: 10,
      get_true_prototype: 11,
      get_false_prototype: 12,
      get_method_prototype: 13,
      get_compiled_code_prototype: 14,
      get_true: 15,
      get_false: 16,
      set_local: 17,
      get_local: 18,
      set_literal_const: 19,
      get_literal_const: 20,
      set_literal_attr: 21,
      get_literal_attr: 22,
      set_compiled_code: 23,
      send_literal: 24,
      return: 25,
      goto_if_false: 26,
      goto_if_true: 27,
      goto: 28,
      def_method: 29,
      def_literal_method: 30,
      run_code: 31,
      get_toplevel: 32,
      is_error: 33,
      error_to_string: 34,
      integer_add: 35,
      integer_div: 36,
      integer_mul: 37,
      integer_sub: 38,
      integer_mod: 39,
      integer_to_float: 40,
      integer_to_string: 41,
      integer_bitwise_and: 42,
      integer_bitwise_or: 43,
      integer_bitwise_xor: 44,
      integer_shift_left: 45,
      integer_shift_right: 46,
      integer_smaller: 47,
      integer_greater: 48,
      integer_equals: 49,
      start_thread: 50,
      float_add: 51,
      float_mul: 52,
      float_div: 53,
      float_sub: 54,
      float_mod: 55,
      float_to_integer: 56,
      float_to_string: 57,
      float_smaller: 58,
      float_greater: 59,
      float_equals: 60,
      array_insert: 61,
      array_at: 62,
      array_remove: 63,
      array_length: 64,
      array_clear: 65,
      string_to_lower: 66,
      string_to_upper: 67,
      string_equals: 68,
      string_to_bytes: 69,
      string_from_bytes: 70,
      string_length: 71,
      string_size: 72,
      stdout_write: 73,
      stderr_write: 74,
      stdin_read: 75,
      stdin_read_line: 76,
      file_open: 77,
      file_write: 78,
      file_read: 79,
      file_read_line: 80,
      file_flush: 81,
      file_size: 82,
      file_seek: 83,
      run_file: 84,
      run_file_dynamic: 85,
      send: 86,
      get_self: 87,
      get_binding_prototype: 88,
      get_binding: 89,
      set_const: 90,
      get_const: 91,
      set_attr: 92,
      get_attr: 93,
      literal_const_exists: 94,
      run_literal_code: 95,
      set_prototype: 96,
      get_prototype: 97,
      local_exists: 98,
      get_caller: 99,
      literal_responds_to: 100,
      responds_to: 101,
      literal_attr_exists: 102
    }

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

    def inspect
      "Instruction(#{name}, #{arguments.inspect}, line: #{line}, column: #{column})"
    end
  end
end

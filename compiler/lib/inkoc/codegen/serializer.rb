# frozen_string_literal: true

module Inkoc
  module Codegen
    class Serializer
      SIGNATURE = 'inko'.bytes
      VERSION = 1

      INTEGER_LITERAL = 0
      FLOAT_LITERAL = 1
      STRING_LITERAL = 2

      def generate(code)
        sig = SIGNATURE.map { |num| u8(num) }.join('')

        sig + u8(VERSION) + compiled_code(code)
      end

      def string(str)
        str = str.to_s
        size  = u64(str.bytesize)
        bytes = str.bytes.pack('C*')

        size + bytes
      end

      def u8(num)
        [num].pack('C')
      end

      def u16(num)
        [num].pack('S>')
      end

      def u32(num)
        [num].pack('L>')
      end

      def u64(num)
        [num].pack('Q>')
      end

      def i32(num)
        [num].pack('l>')
      end

      def i64(num)
        [num].pack('q>')
      end

      def f64(num)
        [num].pack('G')
      end

      def boolean(val)
        u8(val ? 1 : 0)
      end

      def array(values, encoder)
        values = values.map { |value| send(encoder, value) }
        size = u64(values.length)

        size + values.join('')
      end

      def instruction(ins)
        u8(ins.index) +
          array(ins.arguments, :u16) +
          u16(ins.line)
      end

      def catch_entry(entry)
        u16(entry.start) +
          u16(entry.stop) +
          u16(entry.jump_to) +
          u16(entry.register)
      end

      def integer_literal(value)
        u8(INTEGER_LITERAL) + i64(value)
      end

      def float_literal(value)
        u8(FLOAT_LITERAL) + f64(value)
      end

      def string_literal(value)
        u8(STRING_LITERAL) + string(value)
      end

      def literal(value)
        case value
        when Integer
          integer_literal(value)
        when Float
          float_literal(value)
        when String, Symbol
          string_literal(value)
        else
          raise TypeError, "Unsupported literal type: #{value.inspect}"
        end
      end

      # rubocop: disable Metrics/AbcSize
      def compiled_code(code)
        string(code.name) +
          string(code.file.to_s) +
          u16(code.line) +
          u8(code.arguments) +
          u8(code.required_arguments) +
          boolean(code.rest_argument) +
          u16(code.locals) +
          u16(code.registers) +
          boolean(code.captures) +
          array(code.instructions, :instruction) +
          array(code.literals.to_a, :literal) +
          array(code.code_objects.to_a, :compiled_code) +
          array(code.catch_table.to_a, :catch_entry)
      end
    end
  end
end

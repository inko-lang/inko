# frozen_string_literal: true

module Inkoc
  module Codegen
    class Serializer
      SIGNATURE = 'inko'.bytes
      VERSION = 1

      INTEGER_LITERAL = 0
      FLOAT_LITERAL = 1
      STRING_LITERAL = 2
      BIGINT_LITERAL = 3

      U8_RANGE = 0..255
      U16_RANGE = 0..65535
      U32_RANGE = 0..4294967295
      U64_RANGE = 0..18446744073709551615

      I32_RANGE = -2147483648..2147483647
      I64_RANGE = -9223372036854775808..9223372036854775807

      # The range of values that can be encoded as an signed 64 bits integer.
      #
      # These values are based on Rust's `std::i64::MIN` and `std::i64::MAX`.
      INTEGER_RANGE = -9_223_372_036_854_775_808..9_223_372_036_854_775_807

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
        validate_range!(num, U8_RANGE)

        [num].pack('C')
      end

      def u16(num)
        validate_range!(num, U16_RANGE)

        [num].pack('S>')
      end

      def u32(num)
        validate_range!(num, U32_RANGE)

        [num].pack('L>')
      end

      def u64(num)
        validate_range!(num, U64_RANGE)

        [num].pack('Q>')
      end

      def i32(num)
        validate_range!(num, I32_RANGE)

        [num].pack('l>')
      end

      def i64(num)
        validate_range!(num, I64_RANGE)

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
        output = u8(ins.index) + u8(ins.arguments.length)

        ins.arguments.each do |arg|
          output += u16(arg)
        end

        output + u16(ins.line)
      end

      def catch_entry(entry)
        u16(entry.start) +
          u16(entry.stop) +
          u16(entry.jump_to)
      end

      def integer_literal(value)
        if INTEGER_RANGE.cover?(value)
          u8(INTEGER_LITERAL) + i64(value)
        else
          bigint_literal(value)
        end
      end

      def bigint_literal(value)
        u8(BIGINT_LITERAL) + string(value.to_s(16))
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
          array(code.arguments, :literal) +
          u8(code.required_arguments) +
          u16(code.locals) +
          u16(code.registers) +
          boolean(code.captures) +
          array(code.instructions, :instruction) +
          array(code.literals.to_a, :literal) +
          array(code.code_objects.to_a, :compiled_code) +
          array(code.catch_table.to_a, :catch_entry)
      end

      def validate_range!(value, range)
        return if range.cover?(value)

        raise(
          ArgumentError,
          "The value #{value.inspect} is not in the range #{range.inspect}"
        )
      end
    end
  end
end

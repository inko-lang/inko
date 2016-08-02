module Aeon
  class Generator
    SIGNATURE = 'aeon'.bytes
    VERSION   = 1

    def generate(code)
      sig = SIGNATURE.map { |num| u8(num) }.join('')

      sig + u8(VERSION) + compiled_code(code)
    end

    def string(str)
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
      size   = u64(values.length)

      size + values.join('')
    end

    def instruction(ins)
      u16(ins.name_integer) +
        array(ins.arguments, :u32) +
        u32(ins.line) +
        u32(ins.column)
    end

    def compiled_code(code)
      string(code.name) +
        string(code.file) +
        u32(code.line) +
        i32(code.arguments) +
        u32(code.required_arguments) +
        boolean(code.rest_argument) +
        array(code.locals.to_a, :string) +
        array(code.instructions, :instruction) +
        array(code.integers.to_a, :i64) +
        array(code.floats.to_a, :f64) +
        array(code.strings.to_a, :string) +
        array(code.code_objects.to_a, :compiled_code)
    end
  end
end

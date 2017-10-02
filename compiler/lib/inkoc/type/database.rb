# frozen_string_literal: true

module Inkoc
  module Type
    class Database
      include Inspect

      attr_reader :top_level, :block_prototype, :integer_prototype,
                  :float_prototype, :string_prototype, :array_prototype,
                  :boolean_prototype, :nil_prototype, :integer_type,
                  :float_type, :string_type, :boolean_type, :nil_type,
                  :hash_map_prototype

      def initialize
        @top_level = Object.new(name: 'Inko')
        @block_prototype = Object.new(name: 'Block')
        @integer_prototype = Object.new(name: 'Integer')
        @float_prototype = Object.new(name: 'Float')
        @string_prototype = Object.new(name: 'String')
        @array_prototype = Object.new(name: 'Array')
        @boolean_prototype = Object.new(name: 'Boolean')
        @nil_prototype = Object.new(name: 'Nil')
        @hash_map_prototype = Object.new(name: 'HashMap')

        # Instances of these types are immutable so we don't need to allocate
        # new objects every time.
        @integer_type = immutable_object('Integer', @integer_prototype)
        @float_type = immutable_object('Float', @float_prototype)
        @string_type = immutable_object('String', @string_prototype)
        @boolean_type = immutable_object('Boolean', @boolean_prototype)
        @nil_type = immutable_object('Nil', @nil_prototype)
      end

      def immutable_object(name, prototype)
        Object.new(name: name, prototype: prototype).freeze
      end
    end
  end
end

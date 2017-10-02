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
        @top_level = Object.new('Inko')
        @block_prototype = Object.new('Block')
        @integer_prototype = Object.new('Integer')
        @float_prototype = Object.new('Float')
        @string_prototype = Object.new('String')
        @array_prototype = Object.new('Array')
        @boolean_prototype = Object.new('Boolean')
        @nil_prototype = Object.new('Nil')
        @hash_map_prototype = Object.new('HashMap')

        # Instances of these types are immutable so we don't need to allocate
        # new objects every time.
        @integer_type = Object.new('Integer', @integer_prototype).freeze
        @float_type = Object.new('Float', @float_prototype).freeze
        @string_type = Object.new('String', @string_prototype).freeze
        @boolean_type = Object.new('Boolean', @boolean_prototype).freeze
        @nil_type = Object.new('Nil', @nil_prototype).freeze
      end
    end
  end
end

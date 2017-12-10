# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Binary
        include Inspect
        include Predicates

        attr_reader :name, :register, :base, :other, :location

        def initialize(name, register, base, other, location)
          @name = name
          @register = register
          @base = base
          @other = other
          @location = location
        end

        def visitor_method
          :on_binary
        end
      end
    end
  end
end

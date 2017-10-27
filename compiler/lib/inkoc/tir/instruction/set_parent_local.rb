# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetParentLocal
        include Predicates
        include Inspect

        attr_reader :variable, :depth, :value, :location

        def initialize(variable, depth, value, location)
          @variable = variable
          @depth = depth
          @value = value
          @location = location
        end

        def register
          value
        end

        def visitor_method
          :on_set_parent_local
        end
      end
    end
  end
end

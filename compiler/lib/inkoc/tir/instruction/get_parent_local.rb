# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetParentLocal
        include Predicates
        include Inspect

        attr_reader :register, :depth, :variable, :location

        def initialize(register, depth, variable, location)
          @register = register
          @depth = depth
          @variable = variable
          @location = location
        end

        def visitor_method
          :on_get_parent_local
        end
      end
    end
  end
end

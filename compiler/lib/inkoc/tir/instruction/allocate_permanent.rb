# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class AllocatePermanent
        include Predicates
        include Inspect

        attr_reader :register, :prototype, :location

        def initialize(register, prototype, location)
          @register = register
          @prototype = prototype
          @location = location
        end

        def visitor_method
          :on_allocate_permanent
        end
      end
    end
  end
end

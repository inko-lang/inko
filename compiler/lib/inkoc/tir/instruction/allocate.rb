# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Allocate
        include Predicates
        include Inspect

        attr_reader :register, :permanent, :prototype, :location

        def initialize(register, permanent, prototype, location)
          @register = register
          @permanent = permanent
          @prototype = prototype
          @location = location
        end

        def visitor_method
          :on_allocate
        end
      end
    end
  end
end

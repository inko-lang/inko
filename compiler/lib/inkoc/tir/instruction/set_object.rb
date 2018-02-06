# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetObject
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
          :on_set_object
        end
      end
    end
  end
end

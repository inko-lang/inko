# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetPrototype
        include Predicates
        include Inspect

        attr_reader :object, :prototype, :location

        def initialize(object, prototype, location)
          @object = object
          @prototype = prototype
          @location = location
        end

        def register
          prototype
        end

        def visitor_method
          :on_set_prototype
        end
      end
    end
  end
end

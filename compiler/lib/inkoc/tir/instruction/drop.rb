# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Drop
        include Inspect
        include Predicates

        attr_reader :object, :location

        def initialize(object, location)
          @object = object
          @location = location
        end

        def register
          object
        end

        def visitor_method
          :on_drop
        end
      end
    end
  end
end

# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ObjectEquals
        include Inspect
        include Predicates

        attr_reader :register, :object, :compare_with, :location

        def initialize(register, object, compare_with, location)
          @register = register
          @object = object
          @compare_with = compare_with
          @location = location
        end

        def visitor_method
          :on_object_equals
        end
      end
    end
  end
end

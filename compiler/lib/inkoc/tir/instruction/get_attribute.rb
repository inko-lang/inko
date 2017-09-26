# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class GetAttribute
        include Predicates
        include Inspect

        attr_reader :register, :receiver, :name, :location

        def initialize(register, receiver, name, location)
          @register = register
          @receiver = receiver
          @name = name
          @location = location
        end

        def visitor_method
          :on_get_attribute
        end
      end
    end
  end
end
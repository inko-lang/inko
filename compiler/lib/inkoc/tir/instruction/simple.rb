# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Simple
        include Predicates
        include Inspect

        attr_reader :name, :location

        def initialize(name, location)
          @name = name
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_simple
        end
      end
    end
  end
end

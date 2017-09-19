# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetHashMap
        include Predicates
        include Inspect

        attr_reader :register, :pairs, :location

        def initialize(register, pairs, location)
          @register = register
          @pairs = pairs
          @location = location
        end

        def visitor_method
          :on_set_hash_map
        end
      end
    end
  end
end

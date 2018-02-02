# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class MoveToPool
        include Inspect
        include Predicates

        attr_reader :pool_id, :location

        def initialize(pool_id, location)
          @pool_id = pool_id
          @location = location
        end

        def register
          pool_id
        end

        def visitor_method
          :on_move_to_pool
        end
      end
    end
  end
end

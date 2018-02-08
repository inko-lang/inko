# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Exit
        include Inspect
        include Predicates

        attr_reader :status, :location

        def initialize(status, location)
          @status = status
          @location = location
        end

        def register
          status
        end

        def visitor_method
          :on_exit
        end
      end
    end
  end
end

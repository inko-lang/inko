# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class Panic
        include Inspect
        include Predicates

        attr_reader :message, :location

        def initialize(message, location)
          @message = message
          @location = location
        end

        def register
          message
        end

        def visitor_method
          :on_panic
        end
      end
    end
  end
end

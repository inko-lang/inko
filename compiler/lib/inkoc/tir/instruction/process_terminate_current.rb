# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class ProcessTerminateCurrent
        include Inspect
        include Predicates

        attr_reader :location

        def initialize(location)
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_process_terminate_current
        end
      end
    end
  end
end

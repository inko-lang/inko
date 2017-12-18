# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class TailCall
        include Predicates
        include Inspect

        attr_reader :arguments, :keyword_arguments, :location

        def initialize(arguments, keyword_arguments, location)
          @arguments = arguments
          @keyword_arguments = keyword_arguments
          @location = location
        end

        def visitor_method
          :on_tail_call
        end
      end
    end
  end
end

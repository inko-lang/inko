# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class RunBlock
        include Predicates
        include Inspect

        attr_reader :register, :block, :arguments, :keyword_arguments,
                    :location, :block_type

        def initialize(
          register,
          block,
          arguments,
          keyword_arguments,
          block_type,
          location
        )
          @register = register
          @block = block
          @arguments = arguments
          @keyword_arguments = keyword_arguments
          @block_type = block_type
          @location = location
        end

        def run_block?
          true
        end

        def visitor_method
          :on_run_block
        end
      end
    end
  end
end

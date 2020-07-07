# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      module Predicates
        def return?
          false
        end

        def run_block?
          false
        end

        def move_result?
          false
        end
      end
    end
  end
end

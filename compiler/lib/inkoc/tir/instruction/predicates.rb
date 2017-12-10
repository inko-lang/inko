# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      module Predicates
        def return?
          false
        end

        def send_object_message?
          false
        end
      end
    end
  end
end

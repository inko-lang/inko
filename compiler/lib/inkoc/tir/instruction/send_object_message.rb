# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SendObjectMessage
        include Predicates
        include Inspect

        attr_reader :register, :receiver, :name, :arguments, :block_type,
                    :location

        def initialize(register, rec, name, arguments, block_type, location)
          @register = register
          @receiver = rec
          @name = name
          @arguments = arguments
          @block_type = block_type
          @location = location
        end

        def visitor_method
          :on_send_object_message
        end

        def send_object_message?
          true
        end
      end
    end
  end
end

# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SendObjectMessage
        include Predicates
        include Inspect

        attr_reader :register, :receiver, :name, :arguments, :location

        def initialize(register, receiver, name, arguments, location)
          @register = register
          @receiver = receiver
          @name = name
          @arguments = arguments
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

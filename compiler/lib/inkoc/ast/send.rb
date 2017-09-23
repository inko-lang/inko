# frozen_string_literal: true

module Inkoc
  module AST
    class Send
      include Predicates
      include Inspect

      attr_reader :name, :receiver, :arguments, :location

      # name - The name of the message as a String.
      # receiver - The object to send the message to.
      # arguments - The arguments to pass.
      # location - The SourceLocation of the message send.
      def initialize(name, receiver, arguments, location)
        @name = name
        @receiver = receiver
        @arguments = arguments
        @location = location
      end

      def visitor_method
        :on_send
      end

      def raw_instruction?
        receiver &&
          receiver.constant? &&
          receiver.name == Config::RAW_INSTRUCTION_RECEIVER
      end
    end
  end
end

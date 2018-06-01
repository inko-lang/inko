# frozen_string_literal: true

module Inkoc
  module AST
    class Send
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :receiver, :arguments, :location

      attr_accessor :receiver_type, :block_type

      # name - The name of the message as a String.
      # receiver - The object to send the message to.
      # arguments - The arguments to pass.
      # location - The SourceLocation of the message send.
      def initialize(name, receiver, arguments, location)
        @name = name
        @receiver = receiver
        @arguments = arguments
        @location = location
        @receiver_type = nil
        @method_type = nil
      end

      def send?
        true
      end

      def visitor_method
        if raw_instruction?
          :on_raw_instruction
        else
          :on_send
        end
      end

      def raw_instruction?
        receiver&.constant? &&
          receiver&.name == Config::RAW_INSTRUCTION_RECEIVER
      end

      def array_literal?
        receiver&.global? &&
          receiver&.name == Config::ARRAY_CONST &&
          name == Config::NEW_MESSAGE
      end

      def hash_map_literal?
        receiver&.global? &&
          receiver&.name == Config::HASH_MAP_CONST &&
          name == Config::FROM_ARRAY_MESSAGE &&
          arguments[0]&.array_literal? &&
          arguments[1]&.array_literal?
      end

      def raw_instruction_visitor_method
        :"on_raw_#{name}"
      end
    end
  end
end

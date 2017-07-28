# frozen_string_literal: true

module Inkoc
  module AST
    class Method
      include Inspect

      attr_reader :name, :arguments, :type_arguments, :return_type, :throw_type,
                  :body, :location

      # name - The name of the method.
      # args - The arguments of the method.
      # rtype - The return type of the method.
      # throw_type - The type being thrown by this method.
      # body - The body of the method.
      # loc - The SourceLocation of this method.
      def initialize(name, args, targs, rtype, throw_type, body, loc)
        @name = name
        @arguments = args
        @type_arguments = targs
        @return_type = rtype
        @throw_type = throw_type
        @body = body
        @location = loc
      end

      def required?
        @body.nil?
      end

      def tir_process_node_method
        :on_method
      end
    end
  end
end

# frozen_string_literal: true

module Inkoc
  module Type
    class RequiredMethod
      include Inspect

      attr_reader :name, :arguments, :type_parameters, :rest_argument, :throws,
                  :returns

      def initialize(name)
        @name = name
        @arguments = SymbolTable.new
        @type_parameters = SymbolTable.new
        @rest_argument = false
        @throws = nil
        @returns = nil
      end

      def return_type
        returns
      end
    end
  end
end

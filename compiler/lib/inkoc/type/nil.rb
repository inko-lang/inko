# frozen_string_literal: true

module Inkoc
  module Type
    class Nil
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType

      def type_name
        'Nil'
      end
    end
  end
end

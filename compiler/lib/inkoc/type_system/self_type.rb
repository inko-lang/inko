# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class SelfType
      include Type

      def resolve_self_type(self_type)
        self_type
      end

      def type_name
        'Self'
      end
    end
  end
end

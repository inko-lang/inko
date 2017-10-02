# frozen_string_literal: true

module Inkoc
  module Type
    class SelfType
      include Inspect
      include Predicates

      def type_name
        'Self'
      end

      def self_type?
        true
      end
    end
  end
end

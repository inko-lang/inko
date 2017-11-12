# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Type::Constraint do
  let(:constraint) { described_class.new }
  let(:typedb) { Inkoc::Type::Database.new }

  describe '#resolve_constraint' do
    describe 'using a constraint without any required methods' do
      it 'resolves the constraint to the given type' do
        type = Inkoc::Type::Object.new

        expect(constraint.infer_to(type)).to eq(true)
        expect(constraint.inferred_type).to eq(type)
      end
    end

    describe 'when a required method is not implemented' do
      it 'returns false' do
        self_type = Inkoc::Type::Object.new(name: 'A')
        type = Inkoc::Type::Object.new

        constraint.define_required_method(self_type, 'to_string', [], typedb)

        expect(constraint.infer_to(type)).to eq(false)
      end
    end

    describe 'when a required method is implemented' do
      let(:self_type) { Inkoc::Type::Object.new(name: 'A') }
      let(:string) { Inkoc::Type::Object.new(name: 'String') }
      let(:type) { Inkoc::Type::Object.new }

      let(:method) do
        Inkoc::Type::Block.new(
          name: 'to_string',
          prototype: typedb.block_prototype,
          block_type: :method,
          returns: string
        )
      end

      before do
        type.define_attribute(method.name, method)

        constraint.define_required_method(self_type, 'to_string', [], typedb)
      end

      it 'resolves the constraint to the given type' do
        expect(constraint.infer_to(type)).to eq(true)
        expect(constraint.inferred_type).to eq(type)
      end

      it "resolves the required method's return type" do
        constraint.infer_to(type)

        method = constraint.required_methods['to_string']

        expect(method.return_type.inferred_type).to eq(string)
      end
    end
  end
end
